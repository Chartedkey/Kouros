//! Minimal Garmin FIT decoder: file_id, session, sport, record and set messages.
//! Handles compressed-timestamp headers, both endiannesses and developer fields.

use super::{Parsed, Sample, SetRow, Streams};
use std::collections::HashMap;

const FIT_EPOCH: i64 = 631_065_600; // 1989-12-31T00:00:00Z

#[derive(Clone, Copy, Debug, PartialEq)]
enum V {
    U(u64),
    I(i64),
    F(f64),
}

impl V {
    fn f(self) -> f64 {
        match self {
            V::U(x) => x as f64,
            V::I(x) => x as f64,
            V::F(x) => x,
        }
    }
    fn i(self) -> i64 {
        match self {
            V::U(x) => x as i64,
            V::I(x) => x,
            V::F(x) => x as i64,
        }
    }
}

#[derive(Clone, Debug)]
enum Val {
    Num(V),
    Arr(Vec<Option<V>>),
    Str(String),
}

struct FieldDef {
    num: u8,
    size: usize,
    base: u8,
}

struct Def {
    global: u16,
    big: bool,
    fields: Vec<FieldDef>,
    dev_size: usize,
}

type Msg = (u16, HashMap<u8, Val>);

fn read_num(raw: &[u8], base: u8, big: bool) -> Option<V> {
    let u = |n: usize| -> u64 {
        let mut v: u64 = 0;
        for i in 0..n {
            let b = if big { raw[i] } else { raw[n - 1 - i] };
            v = (v << 8) | b as u64;
        }
        v
    };
    let (v, invalid) = match base {
        0x00 | 0x02 | 0x0A | 0x0D => (V::U(u(1)), if base == 0x0A { Some(0) } else { Some(0xFF) }),
        0x01 => (V::I(raw[0] as i8 as i64), Some(0x7F)),
        0x03 => (V::I(u(2) as u16 as i16 as i64), Some(0x7FFF)),
        0x04 | 0x0B => (V::U(u(2)), if base == 0x0B { Some(0) } else { Some(0xFFFF) }),
        0x05 => (V::I(u(4) as u32 as i32 as i64), Some(0x7FFF_FFFF)),
        0x06 | 0x0C => (V::U(u(4)), if base == 0x0C { Some(0) } else { Some(0xFFFF_FFFF) }),
        0x08 => {
            let f = f32::from_bits(u(4) as u32);
            return if f.is_finite() { Some(V::F(f as f64)) } else { None };
        }
        0x09 => {
            let f = f64::from_bits(u(8));
            return if f.is_finite() { Some(V::F(f)) } else { None };
        }
        0x0E => (V::I(u(8) as i64), Some(0x7FFF_FFFF_FFFF_FFFF)),
        0x0F | 0x10 => (V::U(u(8)), if base == 0x10 { Some(0) } else { Some(u64::MAX) }),
        _ => (V::U(u(1)), None),
    };
    let raw_u = match v {
        V::U(x) => x,
        V::I(x) => x as u64 & match base {
            0x01 => 0xFF,
            0x03 => 0xFFFF,
            0x05 => 0xFFFF_FFFF,
            _ => u64::MAX,
        },
        V::F(_) => 0,
    };
    if invalid == Some(raw_u) {
        None
    } else {
        Some(v)
    }
}

fn base_size(base: u8) -> usize {
    match base {
        0x03 | 0x04 | 0x0B => 2,
        0x05 | 0x06 | 0x08 | 0x0C => 4,
        0x09 | 0x0E | 0x0F | 0x10 => 8,
        _ => 1,
    }
}

fn decode(b: &[u8]) -> Result<Vec<Msg>, String> {
    if b.len() < 12 {
        return Err("файл слишком короткий".into());
    }
    let hs = b[0] as usize;
    if hs < 12 || b.len() < hs || &b[8..12] != b".FIT" {
        return Err("это не FIT-файл".into());
    }
    let dsize = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    let end = (hs + dsize).min(b.len());
    let mut p = hs;
    let mut defs: HashMap<u8, Def> = HashMap::new();
    let mut out = Vec::new();
    let mut last_ts: Option<u32> = None;
    let need = |p: usize, n: usize| -> Result<(), String> {
        if p + n > end {
            Err("файл обрывается".into())
        } else {
            Ok(())
        }
    };
    while p < end {
        let h = b[p];
        p += 1;
        let local;
        let mut ts_from_header = None;
        if h & 0x80 != 0 {
            local = (h >> 5) & 0x03;
            let off = (h & 0x1F) as u32;
            if let Some(lt) = last_ts {
                let mut ts = (lt & !0x1F) + off;
                if off < (lt & 0x1F) {
                    ts += 0x20;
                }
                last_ts = Some(ts);
                ts_from_header = Some(ts);
            }
        } else if h & 0x40 != 0 {
            local = h & 0x0F;
            let dev = h & 0x20 != 0;
            need(p, 5)?;
            let big = b[p + 1] == 1;
            let global = if big { u16::from_be_bytes([b[p + 2], b[p + 3]]) } else { u16::from_le_bytes([b[p + 2], b[p + 3]]) };
            let n = b[p + 4] as usize;
            p += 5;
            need(p, n * 3)?;
            let mut fields = Vec::with_capacity(n);
            for _ in 0..n {
                fields.push(FieldDef { num: b[p], size: b[p + 1] as usize, base: b[p + 2] & 0x1F });
                p += 3;
            }
            let mut dev_size = 0;
            if dev {
                need(p, 1)?;
                let nd = b[p] as usize;
                p += 1;
                need(p, nd * 3)?;
                for _ in 0..nd {
                    dev_size += b[p + 1] as usize;
                    p += 3;
                }
            }
            defs.insert(local, Def { global, big, fields, dev_size });
            continue;
        } else {
            local = h & 0x0F;
        }
        let def = defs.get(&local).ok_or("нет описания сообщения")?;
        let mut fields: HashMap<u8, Val> = HashMap::new();
        if let Some(ts) = ts_from_header {
            fields.insert(253, Val::Num(V::U(ts as u64)));
        }
        for fd in &def.fields {
            need(p, fd.size)?;
            let raw = &b[p..p + fd.size];
            p += fd.size;
            let val = if fd.base == 0x07 {
                let z = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                Val::Str(String::from_utf8_lossy(&raw[..z]).trim().to_string())
            } else {
                let bs = base_size(fd.base);
                if fd.size == bs {
                    match read_num(raw, fd.base, def.big) {
                        Some(v) => Val::Num(v),
                        None => continue,
                    }
                } else if fd.size % bs == 0 {
                    Val::Arr(raw.chunks(bs).map(|c| read_num(c, fd.base, def.big)).collect())
                } else {
                    continue;
                }
            };
            fields.insert(fd.num, val);
        }
        need(p, def.dev_size)?;
        p += def.dev_size;
        if h & 0x80 == 0 {
            if let Some(Val::Num(v)) = fields.get(&253) {
                last_ts = Some(v.i() as u32);
            }
        }
        out.push((def.global, fields));
    }
    Ok(out)
}

fn num(f: &HashMap<u8, Val>, k: u8) -> Option<f64> {
    match f.get(&k) {
        Some(Val::Num(v)) => Some(v.f()),
        _ => None,
    }
}

fn int(f: &HashMap<u8, Val>, k: u8) -> Option<i64> {
    match f.get(&k) {
        Some(Val::Num(v)) => Some(v.i()),
        _ => None,
    }
}

fn text(f: &HashMap<u8, Val>, k: u8) -> Option<String> {
    match f.get(&k) {
        Some(Val::Str(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn ts(f: &HashMap<u8, Val>, k: u8) -> Option<i64> {
    int(f, k).map(|t| t + FIT_EPOCH)
}

fn semicircles(v: Option<f64>) -> Option<f64> {
    v.map(|x| x * (180.0 / 2_147_483_648.0)).filter(|d| d.abs() <= 180.0 && *d != 0.0)
}

fn manufacturer(m: i64) -> &'static str {
    match m {
        1 => "Garmin",
        23 => "Suunto",
        32 => "Wahoo",
        38 => "Polar",
        69 => "Stages",
        255 => "Development",
        265 => "Strava",
        267 => "Bryton",
        289 => "Hammerhead",
        294 => "COROS",
        _ => "",
    }
}

fn sport(sport: Option<i64>, sub: Option<i64>) -> &'static str {
    match (sport.unwrap_or(0), sub.unwrap_or(0)) {
        (1, _) => "run",
        (2, 58) => "ride_virtual",
        (2, _) => "ride",
        (5, _) => "swim",
        (10, 20) | (4, 20) => "strength",
        (11, _) => "walk",
        (17, _) => "hike",
        (10, 43) | (62, _) => "yoga",
        _ => "workout",
    }
}

pub fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    let msgs = decode(bytes)?;
    let mut p = Parsed::default();
    let mut streams = Streams::default();
    let mut first_rec: Option<i64> = None;
    let mut session_seen = false;
    let mut sport_name: Option<String> = None;
    let mut device: Option<String> = None;
    let mut created: Option<i64> = None;

    for (g, f) in &msgs {
        match *g {
            0 => {
                created = ts(f, 4);
                let man = int(f, 1).map(manufacturer).unwrap_or("");
                device = text(f, 8).or_else(|| if man.is_empty() { None } else { Some(man.to_string()) });
            }
            12 => {
                if sport_name.is_none() {
                    sport_name = text(f, 3);
                }
            }
            18 if !session_seen => {
                session_seen = true;
                p.sport = sport(int(f, 5), int(f, 6)).to_string();
                if let Some(s) = ts(f, 2) {
                    p.start_utc = s;
                }
                p.elapsed_s = num(f, 7).map(|x| x / 1000.0);
                p.moving_s = num(f, 8).map(|x| x / 1000.0);
                p.distance_m = num(f, 9).map(|x| x / 100.0).filter(|d| *d > 0.0);
                p.calories = num(f, 11);
                p.avg_hr = num(f, 16);
                p.max_hr = num(f, 17);
                p.avg_cadence = num(f, 18);
                p.avg_power = num(f, 20);
                p.elev_gain_m = num(f, 22);
                if sport_name.is_none() {
                    sport_name = text(f, 110);
                }
            }
            20 => {
                let Some(t) = ts(f, 253) else { continue };
                first_rec.get_or_insert(t);
                let alt = num(f, 78).or_else(|| num(f, 2)).map(|a| a / 5.0 - 500.0);
                let speed = num(f, 73).or_else(|| num(f, 6)).map(|s| s / 1000.0);
                streams.push(
                    t,
                    Sample {
                        hr: num(f, 3).filter(|h| *h > 0.0),
                        dist: num(f, 5).map(|d| d / 100.0),
                        speed,
                        alt,
                        cad: num(f, 4),
                        pwr: num(f, 7),
                        lat: semicircles(num(f, 0)),
                        lng: semicircles(num(f, 1)),
                    },
                );
            }
            225 => {
                let weight = num(f, 4).map(|w| w / 16.0).filter(|w| *w > 0.0);
                let category = match f.get(&7) {
                    Some(Val::Arr(a)) => a.iter().flatten().next().map(|v| v.i()),
                    Some(Val::Num(v)) => Some(v.i()),
                    _ => None,
                }
                .filter(|c| *c < 0xFFFE);
                p.sets.push(SetRow {
                    start_utc: ts(f, 6),
                    duration_s: num(f, 0).map(|d| d / 1000.0),
                    reps: int(f, 3),
                    weight_kg: weight,
                    kind: if int(f, 5) == Some(0) { "rest".into() } else { "active".into() },
                    category,
                });
            }
            _ => {}
        }
    }
    if !session_seen {
        p.sport = "workout".into();
        p.start_utc = first_rec.or(created).ok_or("в файле нет ни сессии, ни записей")?;
    }
    if p.start_utc == 0 {
        p.start_utc = first_rec.or(created).unwrap_or(0);
    }
    // Store sample times as seconds from the start.
    let t0 = p.start_utc;
    streams.t.iter_mut().for_each(|t| *t -= t0);
    p.streams = streams;
    p.sport_label = sport_name;
    p.device = device;
    let active: Vec<&SetRow> = p.sets.iter().filter(|s| s.kind == "active").collect();
    if !active.is_empty() {
        p.total_sets = Some(active.len() as i64);
        p.total_reps = Some(active.iter().filter_map(|s| s.reps).sum());
        let lifted: f64 = active.iter().map(|s| s.weight_kg.unwrap_or(0.0) * s.reps.unwrap_or(0) as f64).sum();
        if lifted > 0.0 {
            p.weight_lifted_kg = Some(lifted);
        }
    }
    p.fill_from_streams();
    Ok(p)
}
