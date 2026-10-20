//! Lab report tables from PDF text runs (0.20), std only. First tuned on KDL reports: a header
//! «Наименование исследования | Результат | Ед. измерения | Норм. значения», long names wrapped
//! over 2–3 lines around the value, «+» / «−» after out-of-range values, comments in a smaller
//! font under some rows. Other labs: a looser line-by-line reading.
//! Personal header fields (name, birth date, order number) are never returned.

use super::pdf::Run;

#[derive(Debug, Clone, Default)]
pub struct Row {
    pub name: String,
    pub value_text: String,
    pub value: Option<f64>,
    /// "<" or ">" when the result is «<1.00» / «>1000».
    pub cmp: String,
    /// "+" above, "-" below the lab's range (as marked in the report).
    pub flag: String,
    pub unit: String,
    pub ref_text: String,
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    pub section: String,
    pub comment: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Sampling date, YYYY-MM-DD.
    pub date: Option<String>,
    pub lab: String,
    pub rows: Vec<Row>,
}

fn num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").replace(' ', "").parse::<f64>().ok()
}

/// «4.0-5.5», «<40.0», «> 1.0», «0.5 - 4.0», «до 5» → (lo, hi).
pub fn parse_ref(t: &str) -> (Option<f64>, Option<f64>) {
    let s = t.trim().replace(',', ".").replace('–', "-").replace('—', "-");
    let s = s.trim();
    if s.is_empty() {
        return (None, None);
    }
    let low = s.to_lowercase();
    if let Some(r) = s.strip_prefix("<=").or_else(|| s.strip_prefix('<')).or_else(|| s.strip_prefix('≤')).or_else(|| low.strip_prefix("до ")) {
        return (None, num(r));
    }
    if let Some(r) = s.strip_prefix(">=").or_else(|| s.strip_prefix('>')).or_else(|| s.strip_prefix('≥')).or_else(|| low.strip_prefix("от ")) {
        return (num(r), None);
    }
    // "a-b" (a may be negative only at the very start)
    let body = s.trim_start_matches('-');
    if let Some(i) = body.find('-') {
        let off = s.len() - body.len();
        let (a, b) = (&s[..off + i], &s[off + i + 1..]);
        if let (Some(x), Some(y)) = (num(a), num(b)) {
            return (Some(x), Some(y));
        }
    }
    (None, None)
}

/// «10.5 +», «0.0 -», «<1.00», «55.0» → (text, value, cmp, flag).
pub fn parse_value(t: &str) -> (String, Option<f64>, String, String) {
    let mut s = t.trim().to_string();
    let mut flag = String::new();
    for f in ["+", "-", "↑", "↓", "*", "H", "L"] {
        if s.len() > 1 && s.ends_with(f) && s[..s.len() - f.len()].trim_end().chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            flag = match f {
                "+" | "↑" | "H" => "+".into(),
                "-" | "↓" | "L" => "-".into(),
                _ => String::new(),
            };
            s = s[..s.len() - f.len()].trim_end().to_string();
            break;
        }
    }
    let mut cmp = String::new();
    let mut v = s.as_str();
    if let Some(r) = v.strip_prefix('<') {
        cmp = "<".into();
        v = r;
    } else if let Some(r) = v.strip_prefix('>') {
        cmp = ">".into();
        v = r;
    }
    (s.clone(), num(v), cmp, flag)
}

fn ru_date(t: &str) -> Option<String> {
    // dd.mm.yyyy anywhere in the text
    let b = t.as_bytes();
    for i in 0..b.len().saturating_sub(9) {
        let w = &t[i..];
        if w.len() >= 10 {
            let (d, m, y) = (w.get(0..2)?, w.get(3..5)?, w.get(6..10)?);
            if w.as_bytes()[2] == b'.' && w.as_bytes()[5] == b'.' && d.chars().all(|c| c.is_ascii_digit()) && m.chars().all(|c| c.is_ascii_digit()) && y.chars().all(|c| c.is_ascii_digit()) {
                return Some(format!("{y}-{m}-{d}"));
            }
        }
    }
    None
}

fn is_number_like(t: &str) -> bool {
    let (_, v, _, _) = parse_value(t);
    v.is_some()
}

/// Joins wrapped name lines: «пиридоксаль-5-» + «фосфат)» without a space.
fn join_name(parts: &[String]) -> String {
    let mut out = String::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(p);
        } else if out.ends_with('-') || out.ends_with('(') {
            out.push_str(p);
        } else {
            out.push(' ');
            out.push_str(p);
        }
    }
    out.replace(" ,", ",").replace("( ", "(").replace(" )", ")")
}

struct Cols {
    y: f64,
    res: f64,
    unit: f64,
    refx: f64,
}

/// Reads a report from the runs of a text PDF.
pub fn read(runs: &[Run]) -> Report {
    let mut rep = Report { lab: String::new(), ..Default::default() };
    let all: String = runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ");
    let low = all.to_lowercase();
    rep.lab = if low.contains("kdl") || low.contains("кдл") || (low.contains("лпу:") && low.contains("норм. значения")) {
        "KDL".into()
    } else if low.contains("инвитро") || low.contains("invitro") {
        "Инвитро".into()
    } else if low.contains("гемотест") {
        "Гемотест".into()
    } else if low.contains("хеликс") || low.contains("helix") {
        "Хеликс".into()
    } else if low.contains("cmd") || low.contains("цмд") {
        "CMD".into()
    } else {
        String::new()
    };
    // Sampling date: the run right of «Дата взятия образца» (else of «Дата заказа»).
    for key in ["Дата взятия", "Дата забора", "Взятие биоматериала", "Дата заказа", "Дата выполнения"] {
        if rep.date.is_some() {
            break;
        }
        if let Some(lbl) = runs.iter().find(|r| r.text.starts_with(key)) {
            let same: Option<&Run> = runs.iter().filter(|r| r.page == lbl.page && (r.y - lbl.y).abs() < 4.0 && r.x > lbl.x).min_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
            rep.date = same.and_then(|r| ru_date(&r.text)).or_else(|| ru_date(&lbl.text));
        }
    }
    let pages = runs.iter().map(|r| r.page).max().map(|m| m + 1).unwrap_or(0);
    // Typical font size of test names: the most common size of left-column text that shares a line
    // with something in the middle of the page (a result), over the whole report.
    let name_size_all = {
        let mut sizes: Vec<f64> = runs
            .iter()
            .filter(|r| r.x < 120.0 && runs.iter().any(|v| v.page == r.page && (v.y - r.y).abs() < 3.0 && v.x > 200.0 && v.x < 340.0 && is_number_like(&v.text)))
            .map(|r| (r.size * 10.0).round() / 10.0)
            .collect();
        sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut best = (0usize, 10.0f64);
        let mut i = 0;
        while i < sizes.len() {
            let j = sizes[i..].iter().take_while(|s| (**s - sizes[i]).abs() < 0.05).count();
            if j > best.0 {
                best = (j, sizes[i]);
            }
            i += j.max(1);
        }
        best.1
    };
    let mut cols: Option<Cols> = None;
    let mut section = String::new();
    for pg in 0..pages {
        let pr: Vec<&Run> = runs.iter().filter(|r| r.page == pg).collect();
        // Column header of this page (falls back to the previous page's columns).
        let hname = pr.iter().find(|r| r.text.starts_with("Наименование") || r.text.starts_with("Исследование") || r.text.starts_with("Показатель"));
        if let Some(h) = hname {
            let on = |w: &[&str]| pr.iter().filter(|r| (r.y - h.y).abs() < 4.0).find(|r| w.iter().any(|k| r.text.starts_with(k))).map(|r| r.x);
            if let (Some(res), Some(unit)) = (on(&["Результат"]), on(&["Ед", "Единиц"])) {
                let refx = on(&["Норм", "Референс", "Реф"]).unwrap_or(unit + 110.0);
                cols = Some(Cols { y: h.y, res, unit, refx });
            }
        }
        let Some(c) = cols.as_ref() else { continue };
        let head_y = if hname.is_some() { c.y } else { 0.0 };
        let foot_y = pr.iter().filter(|r| r.text.contains("НЕ ЯВЛЯЕТСЯ ДИАГНОЗОМ") || r.text.contains("не является диагнозом")).map(|r| r.y).fold(f64::MAX, f64::min);
        let body: Vec<&Run> = pr.iter().copied().filter(|r| r.y > head_y + 3.0 && r.y < foot_y - 1.0).collect();
        let col = |x: f64| -> usize {
            if x >= c.refx - 8.0 {
                3
            } else if x >= c.unit - 8.0 {
                2
            } else if x >= c.res - 25.0 {
                1
            } else {
                0
            }
        };
        // Result lines: runs in the result column, one line each (the «+» / «−» mark is its own run).
        let mut rc: Vec<&Run> = body.iter().copied().filter(|r| col(r.x) == 1).collect();
        rc.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap().then(a.x.partial_cmp(&b.x).unwrap()));
        let mut res_runs: Vec<(f64, String)> = Vec::new();
        for r in rc {
            match res_runs.last_mut() {
                Some(l) if (l.0 - r.y).abs() < 3.0 => {
                    l.1.push(' ');
                    l.1.push_str(&r.text);
                }
                _ => res_runs.push((r.y, r.text.clone())),
            }
        }
        res_runs.retain(|l| l.1.chars().count() <= 40);
        if res_runs.is_empty() {
            continue;
        }
        let name_size = name_size_all;
        let mut rows: Vec<(f64, Row, Vec<(f64, String)>)> = res_runs
            .iter()
            .map(|v| {
                let (vt, val, cmp, flag) = parse_value(&v.1);
                let on_line = |k: usize| body.iter().filter(|r| col(r.x) == k && (r.y - v.0).abs() < 3.0).map(|r| r.text.clone()).collect::<Vec<_>>().join(" ");
                let unit = on_line(2);
                let ref_text = on_line(3);
                let (lo, hi) = parse_ref(&ref_text);
                (v.0, Row { value_text: vt, value: val, cmp, flag, unit, ref_text, lo, hi, ..Default::default() }, Vec::new())
            })
            .collect();
        // Name fragments (name font) go to the nearest result line; smaller text is a comment of
        // the row above it; larger bold text in the name column is a section title.
        let mut items: Vec<&Run> = body.iter().copied().filter(|r| col(r.x) == 0).collect();
        items.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        let mut sections: Vec<(f64, String)> = Vec::new();
        for r in items {
            if r.size > name_size + 0.8 {
                sections.push((r.y, r.text.trim_end_matches('.').trim().to_string()));
                continue;
            }
            if (r.size - name_size).abs() <= 0.25 {
                let (k, _) = rows.iter().enumerate().map(|(k, x)| (k, (x.0 - r.y).abs())).fold((0, f64::MAX), |a, b| if b.1 < a.1 { b } else { a });
                if (rows[k].0 - r.y).abs() < name_size * 2.6 {
                    rows[k].2.push((r.y, r.text.clone()));
                    continue;
                }
            }
            // A comment: attach to the last row above it.
            if let Some(row) = rows.iter_mut().rev().find(|x| x.0 < r.y) {
                let t = r.text.trim();
                if t != "." && row.1.comment.len() < 16 {
                    row.1.comment.push(t.to_string());
                }
            }
        }
        for (y, mut row, frags) in rows {
            let mut f = frags;
            f.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            row.name = join_name(&f.into_iter().map(|x| x.1).collect::<Vec<_>>());
            if let Some(s) = sections.iter().filter(|s| s.0 < y).last() {
                section = s.1.clone();
            }
            row.section = section.clone();
            if !row.name.is_empty() && (row.value.is_some() || !row.value_text.is_empty()) {
                rep.rows.push(row);
            }
        }
    }
    if rep.rows.is_empty() {
        rep.rows = loose(runs);
    }
    rep
}

/// Other layouts: each text line «Название  10.5  ед.  4.0-5.0» read from its runs.
fn loose(runs: &[Run]) -> Vec<Row> {
    let mut lines: Vec<Vec<&Run>> = Vec::new();
    let mut sorted: Vec<&Run> = runs.iter().collect();
    sorted.sort_by(|a, b| (a.page, a.y).partial_cmp(&(b.page, b.y)).unwrap().then(a.x.partial_cmp(&b.x).unwrap()));
    for r in sorted {
        match lines.last_mut() {
            Some(l) if l[0].page == r.page && (l[0].y - r.y).abs() < 3.0 => l.push(r),
            _ => lines.push(vec![r]),
        }
    }
    let mut out = Vec::new();
    for mut l in lines {
        l.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        if l.len() < 2 {
            continue;
        }
        let name = l[0].text.trim().to_string();
        if name.chars().count() < 2 || !name.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            continue;
        }
        let Some(vi) = l.iter().skip(1).position(|r| is_number_like(&r.text)).map(|i| i + 1) else { continue };
        let (vt, val, cmp, flag) = parse_value(&l[vi].text);
        let unit = l.get(vi + 1).map(|r| r.text.clone()).unwrap_or_default();
        let (unit, ref_text) = if parse_ref(&unit).0.is_some() || parse_ref(&unit).1.is_some() { (String::new(), unit) } else { (unit, l.get(vi + 2).map(|r| r.text.clone()).unwrap_or_default()) };
        let (lo, hi) = parse_ref(&ref_text);
        if lo.is_none() && hi.is_none() && unit.is_empty() {
            continue;
        }
        out.push(Row { name, value_text: vt, value: val, cmp, flag, unit, ref_text, lo, hi, ..Default::default() });
    }
    out
}
