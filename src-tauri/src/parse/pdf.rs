//! Minimal PDF text reader (0.20), std only: enough for lab reports that are real text PDFs
//! (KDL and similar). Reads objects, inflates Flate streams, maps glyphs through ToUnicode, runs
//! the text operators and returns text runs with page positions. No images, no scans.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Arr(Vec<Obj>),
    Dict(HashMap<String, Obj>),
    Ref(u32),
}

impl Obj {
    fn get(&self, k: &str) -> Option<&Obj> {
        match self {
            Obj::Dict(d) => d.get(k),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
    fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
}

/// One piece of text as placed on the page (x, y from the top-left corner, in points).
#[derive(Debug, Clone)]
pub struct Run {
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub font: String,
    pub text: String,
    /// Right edge of the run.
    pub end: f64,
}

// ---------- inflate (RFC 1950/1951) ----------

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.nbits < n {
            let b = *self.d.get(self.pos).ok_or("inflate: unexpected end")? as u32;
            self.pos += 1;
            self.bit |= b << self.nbits;
            self.nbits += 8;
        }
        Ok(())
    }
    fn get(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }
    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

struct Huff {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Huff {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { counts, symbols }
    }
    fn decode(&self, b: &mut Bits) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("inflate: bad code".into())
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn codes(b: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Result<(), String> {
    loop {
        let s = lit.decode(b)? as usize;
        if s < 256 {
            out.push(s as u8);
        } else if s == 256 {
            return Ok(());
        } else {
            let s = s - 257;
            if s >= 29 {
                return Err("inflate: bad length".into());
            }
            let len = LBASE[s] as usize + b.get(LEXT[s] as u32)? as usize;
            let ds = dist.decode(b)? as usize;
            if ds >= 30 {
                return Err("inflate: bad distance".into());
            }
            let d = DBASE[ds] as usize + b.get(DEXT[ds] as u32)? as usize;
            if d > out.len() {
                return Err("inflate: distance too far".into());
            }
            let start = out.len() - d;
            for i in 0..len {
                let c = out[start + i];
                out.push(c);
            }
        }
    }
}

/// zlib stream (or raw deflate if there is no zlib header).
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let raw = if data.len() > 2 && (data[0] & 0x0f) == 8 && ((data[0] as u16) << 8 | data[1] as u16) % 31 == 0 { &data[2..] } else { data };
    let mut b = Bits { d: raw, pos: 0, bit: 0, nbits: 0 };
    let mut out = Vec::with_capacity(raw.len() * 4);
    loop {
        let last = b.get(1)?;
        match b.get(2)? {
            0 => {
                b.align();
                if b.pos + 4 > raw.len() {
                    return Err("inflate: short stored block".into());
                }
                let len = u16::from_le_bytes([raw[b.pos], raw[b.pos + 1]]) as usize;
                b.pos += 4;
                let end = (b.pos + len).min(raw.len());
                out.extend_from_slice(&raw[b.pos..end]);
                b.pos = end;
            }
            1 => {
                let mut l = [0u8; 288];
                for (i, x) in l.iter_mut().enumerate() {
                    *x = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
                }
                let lit = Huff::new(&l);
                let dist = Huff::new(&[5u8; 30]);
                codes(&mut b, &mut out, &lit, &dist)?;
            }
            2 => {
                let hlit = b.get(5)? as usize + 257;
                let hdist = b.get(5)? as usize + 1;
                let hclen = b.get(4)? as usize + 4;
                const ORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in ORD.iter().take(hclen) {
                    cl[o] = b.get(3)? as u8;
                }
                let ch = Huff::new(&cl);
                let mut lens = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let s = ch.decode(&mut b)?;
                    match s {
                        0..=15 => {
                            lens[i] = s as u8;
                            i += 1;
                        }
                        16 => {
                            if i == 0 {
                                return Err("inflate: repeat without length".into());
                            }
                            let p = lens[i - 1];
                            for _ in 0..3 + b.get(2)? {
                                if i < lens.len() {
                                    lens[i] = p;
                                    i += 1;
                                }
                            }
                        }
                        17 => i += 3 + b.get(3)? as usize,
                        _ => i += 11 + b.get(7)? as usize,
                    }
                }
                lens.truncate(hlit + hdist);
                let lit = Huff::new(&lens[..hlit]);
                let dist = Huff::new(&lens[hlit..]);
                codes(&mut b, &mut out, &lit, &dist)?;
            }
            _ => return Err("inflate: bad block type".into()),
        }
        if last == 1 || b.pos >= raw.len() {
            return Ok(out);
        }
    }
}

// ---------- tokenizer / object parser ----------

#[derive(Debug, Clone)]
enum Tok {
    Obj(Obj),
    Op(String),
    ArrOpen,
    ArrClose,
    DictOpen,
    DictClose,
}

struct Lexer<'a> {
    d: &'a [u8],
    p: usize,
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\n' | b'\r' | b'\t' | b'\x0c' | 0)
}
fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

impl<'a> Lexer<'a> {
    fn skip(&mut self) {
        while self.p < self.d.len() {
            let c = self.d[self.p];
            if is_ws(c) {
                self.p += 1;
            } else if c == b'%' {
                while self.p < self.d.len() && self.d[self.p] != b'\n' && self.d[self.p] != b'\r' {
                    self.p += 1;
                }
            } else {
                break;
            }
        }
    }
    fn word(&mut self) -> String {
        let s = self.p;
        while self.p < self.d.len() && !is_ws(self.d[self.p]) && !is_delim(self.d[self.p]) {
            self.p += 1;
        }
        String::from_utf8_lossy(&self.d[s..self.p]).into_owned()
    }
    fn next(&mut self) -> Option<Tok> {
        self.skip();
        if self.p >= self.d.len() {
            return None;
        }
        let c = self.d[self.p];
        match c {
            b'[' => {
                self.p += 1;
                Some(Tok::ArrOpen)
            }
            b']' => {
                self.p += 1;
                Some(Tok::ArrClose)
            }
            b'<' if self.d.get(self.p + 1) == Some(&b'<') => {
                self.p += 2;
                Some(Tok::DictOpen)
            }
            b'>' if self.d.get(self.p + 1) == Some(&b'>') => {
                self.p += 2;
                Some(Tok::DictClose)
            }
            b'<' => {
                self.p += 1;
                let mut hex = Vec::new();
                while self.p < self.d.len() && self.d[self.p] != b'>' {
                    let h = self.d[self.p];
                    if h.is_ascii_hexdigit() {
                        hex.push(h);
                    }
                    self.p += 1;
                }
                self.p += 1;
                if hex.len() % 2 == 1 {
                    hex.push(b'0');
                }
                let v = hex.chunks(2).map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap_or("00"), 16).unwrap_or(0)).collect();
                Some(Tok::Obj(Obj::Str(v)))
            }
            b'(' => {
                self.p += 1;
                let mut depth = 1;
                let mut v = Vec::new();
                while self.p < self.d.len() {
                    let c = self.d[self.p];
                    self.p += 1;
                    match c {
                        b'\\' => {
                            let e = *self.d.get(self.p).unwrap_or(&b'\\');
                            self.p += 1;
                            match e {
                                b'n' => v.push(b'\n'),
                                b'r' => v.push(b'\r'),
                                b't' => v.push(b'\t'),
                                b'b' => v.push(8),
                                b'f' => v.push(12),
                                b'0'..=b'7' => {
                                    let mut n = (e - b'0') as u32;
                                    for _ in 0..2 {
                                        match self.d.get(self.p) {
                                            Some(&d) if (b'0'..=b'7').contains(&d) => {
                                                n = n * 8 + (d - b'0') as u32;
                                                self.p += 1;
                                            }
                                            _ => break,
                                        }
                                    }
                                    v.push(n as u8);
                                }
                                b'\r' | b'\n' => {}
                                other => v.push(other),
                            }
                        }
                        b'(' => {
                            depth += 1;
                            v.push(c);
                        }
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            v.push(c);
                        }
                        _ => v.push(c),
                    }
                }
                Some(Tok::Obj(Obj::Str(v)))
            }
            b'/' => {
                self.p += 1;
                Some(Tok::Obj(Obj::Name(self.word())))
            }
            b'{' | b'}' | b')' | b'>' => {
                self.p += 1;
                self.next()
            }
            _ => {
                let w = self.word();
                if w.is_empty() {
                    self.p += 1;
                    return self.next();
                }
                if let Ok(n) = w.parse::<f64>() {
                    return Some(Tok::Obj(Obj::Num(n)));
                }
                match w.as_str() {
                    "true" => Some(Tok::Obj(Obj::Bool(true))),
                    "false" => Some(Tok::Obj(Obj::Bool(false))),
                    "null" => Some(Tok::Obj(Obj::Null)),
                    _ => Some(Tok::Op(w)),
                }
            }
        }
    }
}

/// Parses one object from tokens (numbers followed by "n 0 R" become references).
fn parse_obj(toks: &[Tok], i: &mut usize) -> Option<Obj> {
    let t = toks.get(*i)?.clone();
    *i += 1;
    match t {
        Tok::Obj(Obj::Num(n)) => {
            if let (Some(Tok::Obj(Obj::Num(_))), Some(Tok::Op(r))) = (toks.get(*i), toks.get(*i + 1)) {
                if r == "R" {
                    *i += 2;
                    return Some(Obj::Ref(n as u32));
                }
            }
            Some(Obj::Num(n))
        }
        Tok::Obj(o) => Some(o),
        Tok::ArrOpen => {
            let mut v = Vec::new();
            while let Some(t) = toks.get(*i) {
                if matches!(t, Tok::ArrClose) {
                    *i += 1;
                    break;
                }
                match parse_obj(toks, i) {
                    Some(o) => v.push(o),
                    None => break,
                }
            }
            Some(Obj::Arr(v))
        }
        Tok::DictOpen => {
            let mut m = HashMap::new();
            while let Some(t) = toks.get(*i) {
                if matches!(t, Tok::DictClose) {
                    *i += 1;
                    break;
                }
                let k = match parse_obj(toks, i) {
                    Some(Obj::Name(k)) => k,
                    Some(_) => continue,
                    None => break,
                };
                if let Some(v) = parse_obj(toks, i) {
                    m.insert(k, v);
                }
            }
            Some(Obj::Dict(m))
        }
        Tok::Op(_) => Some(Obj::Null),
        Tok::ArrClose | Tok::DictClose => None,
    }
}

fn lex_all(d: &[u8]) -> Vec<Tok> {
    let mut l = Lexer { d, p: 0 };
    let mut v = Vec::new();
    while let Some(t) = l.next() {
        v.push(t);
    }
    v
}

// ---------- document ----------

struct Doc {
    objs: HashMap<u32, (Obj, Option<Vec<u8>>)>,
}

fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    if n.is_empty() || from >= h.len() {
        return None;
    }
    h[from..].windows(n.len()).position(|w| w == n).map(|p| p + from)
}

impl Doc {
    fn load(d: &[u8]) -> Doc {
        let mut objs = HashMap::new();
        let mut p = 0;
        while let Some(o) = find(d, b" obj", p) {
            // "<num> <gen> obj"
            let mut s = o;
            let mut spaces = 0;
            while s > 0 {
                let c = d[s - 1];
                if c.is_ascii_digit() {
                    s -= 1;
                } else if c == b' ' && spaces == 0 {
                    spaces += 1;
                    s -= 1;
                } else {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&d[s..o]).into_owned();
            let num: Option<u32> = head.split_whitespace().next().and_then(|x| x.parse().ok());
            let body_start = o + 4;
            let end = find(d, b"endobj", body_start).unwrap_or(d.len());
            if let Some(num) = num {
                let body = &d[body_start..end];
                let (dict_part, stream) = match find(body, b"stream", 0) {
                    Some(sp) if find(&body[..sp], b"endstream", 0).is_none() => {
                        let mut ds = sp + 6;
                        if body.get(ds) == Some(&b'\r') {
                            ds += 1;
                        }
                        if body.get(ds) == Some(&b'\n') {
                            ds += 1;
                        }
                        let de = find(body, b"endstream", ds).unwrap_or(body.len());
                        let mut de2 = de;
                        while de2 > ds && (body[de2 - 1] == b'\n' || body[de2 - 1] == b'\r') {
                            de2 -= 1;
                        }
                        (&body[..sp], Some(body[ds..de2].to_vec()))
                    }
                    _ => (body, None),
                };
                let toks = lex_all(dict_part);
                let mut i = 0;
                let obj = parse_obj(&toks, &mut i).unwrap_or(Obj::Null);
                objs.insert(num, (obj, stream));
            }
            p = end.max(o + 4);
        }
        Doc { objs }
    }
    fn res<'a>(&'a self, o: &'a Obj) -> &'a Obj {
        let mut o = o;
        for _ in 0..8 {
            match o {
                Obj::Ref(n) => match self.objs.get(n) {
                    Some((x, _)) => o = x,
                    None => return &Obj::Null,
                },
                _ => return o,
            }
        }
        o
    }
    fn stream(&self, r: &Obj) -> Option<Vec<u8>> {
        let n = match r {
            Obj::Ref(n) => *n,
            _ => return None,
        };
        let (dict, data) = self.objs.get(&n)?;
        let data = data.as_ref()?;
        let filters: Vec<String> = match self.res(dict.get("Filter").unwrap_or(&Obj::Null)) {
            Obj::Name(f) => vec![f.clone()],
            Obj::Arr(a) => a.iter().filter_map(|x| x.name().map(String::from)).collect(),
            _ => vec![],
        };
        let mut v = data.clone();
        for f in filters {
            match f.as_str() {
                "FlateDecode" | "Fl" => v = inflate(&v).ok()?,
                _ => return None,
            }
        }
        Some(v)
    }
    fn pages(&self) -> Vec<u32> {
        // Through the page tree from the catalog; else every /Type /Page object by number.
        let root = self.objs.values().find(|(o, _)| o.get("Type").and_then(|t| t.name()) == Some("Catalog")).and_then(|(o, _)| o.get("Pages").cloned());
        let mut out = Vec::new();
        fn walk(doc: &Doc, r: &Obj, out: &mut Vec<u32>, depth: usize) {
            if depth > 20 {
                return;
            }
            let o = doc.res(r);
            match o.get("Type").and_then(|t| t.name()) {
                Some("Pages") => {
                    if let Some(Obj::Arr(k)) = o.get("Kids").map(|k| doc.res(k)) {
                        for kid in k {
                            walk(doc, kid, out, depth + 1);
                        }
                    }
                }
                Some("Page") => {
                    if let Obj::Ref(n) = r {
                        out.push(*n);
                    }
                }
                _ => {}
            }
        }
        if let Some(r) = root {
            walk(self, &r, &mut out, 0);
        }
        if out.is_empty() {
            let mut v: Vec<u32> = self.objs.iter().filter(|(_, (o, _))| o.get("Type").and_then(|t| t.name()) == Some("Page")).map(|(n, _)| *n).collect();
            v.sort();
            out = v;
        }
        out
    }
}

// ---------- fonts ----------

struct Font {
    two_byte: bool,
    map: HashMap<u32, String>,
    widths: HashMap<u32, f64>,
    dw: f64,
}

fn hex_code(s: &[u8]) -> u32 {
    s.iter().fold(0u32, |a, b| (a << 8) | *b as u32)
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks(2).map(|c| if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { c[0] as u16 }).collect();
    String::from_utf16_lossy(&u)
}

fn parse_cmap(data: &[u8]) -> HashMap<u32, String> {
    let toks = lex_all(data);
    let mut m = HashMap::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Op(op) if op == "beginbfchar" => {
                i += 1;
                while i + 1 < toks.len() {
                    if let Tok::Op(e) = &toks[i] {
                        if e == "endbfchar" {
                            break;
                        }
                    }
                    if let (Tok::Obj(Obj::Str(a)), Tok::Obj(Obj::Str(b))) = (&toks[i], &toks[i + 1]) {
                        m.insert(hex_code(a), utf16(b));
                    }
                    i += 2;
                }
            }
            Tok::Op(op) if op == "beginbfrange" => {
                i += 1;
                while i + 2 < toks.len() {
                    if let Tok::Op(e) = &toks[i] {
                        if e == "endbfrange" {
                            break;
                        }
                    }
                    if let (Tok::Obj(Obj::Str(a)), Tok::Obj(Obj::Str(b))) = (&toks[i], &toks[i + 1]) {
                        let (lo, hi) = (hex_code(a), hex_code(b));
                        match &toks[i + 2] {
                            Tok::Obj(Obj::Str(dst)) => {
                                let base: Vec<u16> = dst.chunks(2).map(|c| if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { c[0] as u16 }).collect();
                                for (k, code) in (lo..=hi.min(lo + 65535)).enumerate() {
                                    let mut v = base.clone();
                                    if let Some(last) = v.last_mut() {
                                        *last = last.wrapping_add(k as u16);
                                    }
                                    m.insert(code, String::from_utf16_lossy(&v));
                                }
                                i += 3;
                            }
                            Tok::ArrOpen => {
                                let mut j = i + 3;
                                let mut code = lo;
                                while j < toks.len() && !matches!(toks[j], Tok::ArrClose) {
                                    if let Tok::Obj(Obj::Str(s)) = &toks[j] {
                                        m.insert(code, utf16(s));
                                        code += 1;
                                    }
                                    j += 1;
                                }
                                i = j + 1;
                            }
                            _ => i += 3,
                        }
                        continue;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    m
}

/// Windows-1252 for simple fonts without a ToUnicode map.
fn win1252(b: u8) -> char {
    const HI: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&b) {
        char::from_u32(HI[(b - 0x80) as usize] as u32).unwrap_or('?')
    } else {
        b as char
    }
}

impl Font {
    /// Advance of a string in text space units (thousandths of the font size).
    fn width(&self, s: &[u8]) -> f64 {
        if self.two_byte {
            s.chunks(2).map(|c| *self.widths.get(&hex_code(c)).unwrap_or(&self.dw)).sum()
        } else {
            s.iter().map(|b| *self.widths.get(&(*b as u32)).unwrap_or(&self.dw)).sum()
        }
    }
    fn decode(&self, s: &[u8]) -> String {
        let mut out = String::new();
        if self.two_byte {
            for c in s.chunks(2) {
                let code = hex_code(c);
                if let Some(t) = self.map.get(&code) {
                    out.push_str(t);
                }
            }
        } else {
            for &b in s {
                match self.map.get(&(b as u32)) {
                    Some(t) => out.push_str(t),
                    None => out.push(win1252(b)),
                }
            }
        }
        out
    }
}

// ---------- content streams ----------

type M = [f64; 6];

fn mul(a: &M, b: &M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

struct Ctx<'a> {
    doc: &'a Doc,
    fonts_cache: HashMap<u32, std::rc::Rc<Font>>,
    runs: Vec<Run>,
    page: usize,
    height: f64,
}

fn flush(cur: &mut Option<Run>, runs: &mut Vec<Run>) {
    if let Some(r) = cur.take() {
        let t = r.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() {
            runs.push(Run { text: t, ..r });
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Shows a string at the text matrix; returns its advance in text space (to move the matrix).
fn show(font: &Option<std::rc::Rc<Font>>, font_name: &str, size: f64, page: usize, height: f64, s: &[u8], tm: &M, ctm: &M, cur: &mut Option<Run>, runs: &mut Vec<Run>) -> f64 {
    let Some(f) = font else { return 0.0 };
    let text = f.decode(s);
    let adv = f.width(s) / 1000.0 * size;
    let m = mul(tm, ctm);
    let (x, y) = (m[4], height - m[5]);
    let sx = (m[0] * m[0] + m[1] * m[1]).sqrt();
    let fs = size * (m[2] * m[2] + m[3] * m[3]).sqrt();
    let end = x + adv * sx;
    match cur {
        // Same line and close to the previous piece: one run, a space only where there is a gap.
        Some(r) if (r.y - y).abs() < fs * 0.5 && x - r.end < fs * 1.5 && x >= r.x => {
            if x - r.end > fs * 0.18 && !r.text.ends_with(' ') && !text.starts_with(' ') {
                r.text.push(' ');
            }
            r.text.push_str(&text);
            r.end = r.end.max(end);
        }
        _ => {
            flush(cur, runs);
            *cur = Some(Run { page, x, y, size: fs, font: font_name.to_string(), text, end });
        }
    }
    adv
}

impl<'a> Ctx<'a> {
    fn font(&mut self, r: &Obj) -> Option<std::rc::Rc<Font>> {
        let n = match r {
            Obj::Ref(n) => *n,
            _ => 0,
        };
        if n != 0 {
            if let Some(f) = self.fonts_cache.get(&n) {
                return Some(f.clone());
            }
        }
        let fd = self.doc.res(r);
        let two = fd.get("Subtype").and_then(|s| s.name()) == Some("Type0");
        let map = fd.get("ToUnicode").and_then(|t| self.doc.stream(t)).map(|d| parse_cmap(&d)).unwrap_or_default();
        // Glyph widths (thousandths of the font size): /W of the descendant CID font, or /Widths.
        let mut widths = HashMap::new();
        let mut dw = if two { 1000.0 } else { 500.0 };
        if two {
            if let Some(Obj::Arr(ds)) = fd.get("DescendantFonts").map(|d| self.doc.res(d).clone()) {
                if let Some(df) = ds.first().map(|d| self.doc.res(d).clone()) {
                    if let Some(v) = df.get("DW").and_then(|x| x.num()) {
                        dw = v;
                    }
                    if let Some(Obj::Arr(w)) = df.get("W").map(|x| self.doc.res(x).clone()) {
                        let mut i = 0;
                        while i < w.len() {
                            let Some(c1) = w[i].num() else { break };
                            match w.get(i + 1).map(|x| self.doc.res(x).clone()) {
                                Some(Obj::Arr(list)) => {
                                    for (k, x) in list.iter().enumerate() {
                                        if let Some(v) = x.num() {
                                            widths.insert(c1 as u32 + k as u32, v);
                                        }
                                    }
                                    i += 2;
                                }
                                Some(Obj::Num(c2)) => {
                                    let v = w.get(i + 2).and_then(|x| x.num()).unwrap_or(dw);
                                    for c in c1 as u32..=(c2 as u32).min(c1 as u32 + 65535) {
                                        widths.insert(c, v);
                                    }
                                    i += 3;
                                }
                                _ => break,
                            }
                        }
                    }
                }
            }
        } else if let (Some(fc), Some(Obj::Arr(ws))) = (fd.get("FirstChar").and_then(|x| x.num()), fd.get("Widths").map(|x| self.doc.res(x).clone())) {
            for (k, x) in ws.iter().enumerate() {
                if let Some(v) = x.num() {
                    widths.insert(fc as u32 + k as u32, v);
                }
            }
        }
        let f = std::rc::Rc::new(Font { two_byte: two, map, widths, dw });
        if n != 0 {
            self.fonts_cache.insert(n, f.clone());
        }
        Some(f)
    }

    fn run(&mut self, content: &[u8], resources: &Obj, base: M, depth: usize) {
        if depth > 6 {
            return;
        }
        let res = self.doc.res(resources).clone();
        let fonts = res.get("Font").map(|f| self.doc.res(f).clone()).unwrap_or(Obj::Null);
        let xobjs = res.get("XObject").map(|f| self.doc.res(f).clone()).unwrap_or(Obj::Null);
        let toks = lex_all(content);
        let mut stack: Vec<Obj> = Vec::new();
        let mut ctm = base;
        let mut saved: Vec<M> = Vec::new();
        let (mut tm, mut tlm) = (ID, ID);
        let mut font: Option<std::rc::Rc<Font>> = None;
        let mut font_name = String::new();
        let (mut size, mut leading) = (12.0, 0.0);
        // The current BT block's text gathered into one run.
        let mut cur: Option<Run> = None;
        let (page, height) = (self.page, self.height);
        let mut i = 0;
        while i < toks.len() {
            let t = toks[i].clone();
            match t {
                Tok::Op(op) => {
                    let num = |k: usize, st: &Vec<Obj>| -> f64 { st.get(st.len().wrapping_sub(k)).and_then(|o| o.num()).unwrap_or(0.0) };
                    match op.as_str() {
                        "q" => saved.push(ctm),
                        "Q" => ctm = saved.pop().unwrap_or(base),
                        "cm" => {
                            let m = [num(6, &stack), num(5, &stack), num(4, &stack), num(3, &stack), num(2, &stack), num(1, &stack)];
                            ctm = mul(&m, &ctm);
                        }
                        "BT" => {
                            tm = ID;
                            tlm = ID;
                        }
                        "ET" => flush(&mut cur, &mut self.runs),
                        "Tf" => {
                            size = num(1, &stack);
                            if let Some(Obj::Name(n)) = stack.get(stack.len().wrapping_sub(2)) {
                                font_name = n.clone();
                                font = fonts.get(n).cloned().and_then(|r| self.font(&r));
                            }
                        }
                        "TL" => leading = num(1, &stack),
                        "Tm" => {
                            tm = [num(6, &stack), num(5, &stack), num(4, &stack), num(3, &stack), num(2, &stack), num(1, &stack)];
                            tlm = tm;
                        }
                        "Td" | "TD" => {
                            let (tx, ty) = (num(2, &stack), num(1, &stack));
                            if op == "TD" {
                                leading = -ty;
                            }
                            tlm = mul(&[1.0, 0.0, 0.0, 1.0, tx, ty], &tlm);
                            tm = tlm;
                        }
                        "T*" => {
                            tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -leading], &tlm);
                            tm = tlm;
                        }
                        "Tj" | "'" | "\"" => {
                            if op != "Tj" {
                                tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -leading], &tlm);
                                tm = tlm;
                            }
                            if let Some(Obj::Str(s)) = stack.last() {
                                let s = s.clone();
                                let adv = show(&font, &font_name, size, page, height, &s, &tm, &ctm, &mut cur, &mut self.runs);
                                tm = mul(&[1.0, 0.0, 0.0, 1.0, adv, 0.0], &tm);
                            }
                        }
                        "TJ" => {
                            if let Some(Obj::Arr(a)) = stack.last() {
                                let a = a.clone();
                                for el in a {
                                    match el {
                                        Obj::Str(s) => {
                                            let adv = show(&font, &font_name, size, page, height, &s, &tm, &ctm, &mut cur, &mut self.runs);
                                            tm = mul(&[1.0, 0.0, 0.0, 1.0, adv, 0.0], &tm);
                                        }
                                        Obj::Num(n) => {
                                            tm = mul(&[1.0, 0.0, 0.0, 1.0, -n / 1000.0 * size, 0.0], &tm);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "Do" => {
                            if let Some(Obj::Name(n)) = stack.last() {
                                if let Some(r) = xobjs.get(n).cloned() {
                                    let xo = self.doc.res(&r).clone();
                                    if xo.get("Subtype").and_then(|s| s.name()) == Some("Form") {
                                        if let Some(data) = self.doc.stream(&r) {
                                            let fm = match xo.get("Matrix").map(|m| self.doc.res(m)) {
                                                Some(Obj::Arr(a)) if a.len() == 6 => {
                                                    let v: Vec<f64> = a.iter().map(|x| x.num().unwrap_or(0.0)).collect();
                                                    [v[0], v[1], v[2], v[3], v[4], v[5]]
                                                }
                                                _ => ID,
                                            };
                                            let r2 = xo.get("Resources").cloned().unwrap_or_else(|| resources.clone());
                                            flush(&mut cur, &mut self.runs);
                                            self.run(&data, &r2, mul(&fm, &ctm), depth + 1);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    stack.clear();
                }
                _ => {
                    let mut j = i;
                    if let Some(o) = parse_obj(&toks, &mut j) {
                        stack.push(o);
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        flush(&mut cur, &mut self.runs);
    }
}

/// All text runs of the document, page by page, in drawing order.
pub fn text_runs(data: &[u8]) -> Result<Vec<Run>, String> {
    if !data.starts_with(b"%PDF") {
        return Err("Это не PDF-файл".into());
    }
    let doc = Doc::load(data);
    let pages = doc.pages();
    if pages.is_empty() {
        return Err("В PDF не нашлось страниц".into());
    }
    let mut ctx = Ctx { doc: &doc, fonts_cache: HashMap::new(), runs: Vec::new(), page: 0, height: 842.0 };
    for (pi, pn) in pages.iter().enumerate() {
        let page = doc.res(&Obj::Ref(*pn)).clone();
        // Resources and MediaBox can be inherited from the parent.
        let mut node = page.clone();
        let mut resources = page.get("Resources").cloned();
        let mut media = page.get("MediaBox").cloned();
        for _ in 0..10 {
            if resources.is_some() && media.is_some() {
                break;
            }
            let Some(p) = node.get("Parent").cloned() else { break };
            node = doc.res(&p).clone();
            if resources.is_none() {
                resources = node.get("Resources").cloned();
            }
            if media.is_none() {
                media = node.get("MediaBox").cloned();
            }
        }
        ctx.height = match media.as_ref().map(|m| doc.res(m)) {
            Some(Obj::Arr(a)) if a.len() == 4 => a[3].num().unwrap_or(842.0),
            _ => 842.0,
        };
        ctx.page = pi;
        let mut content = Vec::new();
        match page.get("Contents").map(|c| (c.clone(), doc.res(c).clone())) {
            Some((r @ Obj::Ref(_), Obj::Dict(_))) => {
                if let Some(s) = doc.stream(&r) {
                    content = s;
                }
            }
            Some((_, Obj::Arr(a))) => {
                for c in a {
                    if let Some(s) = doc.stream(&c) {
                        content.extend_from_slice(&s);
                        content.push(b'\n');
                    }
                }
            }
            _ => {}
        }
        let res = resources.unwrap_or(Obj::Null);
        ctx.run(&content, &res, ID, 0);
    }
    Ok(ctx.runs)
}
