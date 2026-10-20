//! Date helpers without external crates. All results are Unix seconds (UTC).

/// Days from 1970-01-01 to the given civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn unix(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s
}

/// Parses ISO 8601 like `2026-10-16T18:28:09Z`, `2026-10-16T18:28:09.000Z`,
/// `2026-10-16T21:28:09+03:00` or `2026-10-16 18:28:09`.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let n = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse().ok() };
    let (y, mo, d, h, mi, se) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let mut t = unix(y, mo, d, h, mi, se);
    let mut rest = &s[19..];
    if rest.starts_with('.') {
        let end = rest[1..].find(|c: char| !c.is_ascii_digit()).map(|i| i + 1).unwrap_or(rest.len());
        rest = &rest[end..];
    }
    if rest.len() >= 6 && (rest.starts_with('+') || rest.starts_with('-')) {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let oh: i64 = rest.get(1..3)?.parse().ok()?;
        let om: i64 = rest.get(4..6)?.parse().ok()?;
        t -= sign * (oh * 3600 + om * 60);
    }
    Some(t)
}

const RU_MONTHS: [&str; 12] = ["янв", "фев", "мар", "апр", "ма", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];
const EN_MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

fn month_of(word: &str) -> Option<i64> {
    let w = word.trim_matches(|c: char| c == '.' || c == ',').to_lowercase();
    for (i, p) in EN_MONTHS.iter().enumerate() {
        if w.starts_with(p) {
            return Some(i as i64 + 1);
        }
    }
    // "мая"/"май" share the "ма" stem with "мар" — check март first.
    if w.starts_with("мар") {
        return Some(3);
    }
    for (i, p) in RU_MONTHS.iter().enumerate() {
        if w.starts_with(p) {
            return Some(i as i64 + 1);
        }
    }
    None
}

/// Parses the Strava archive date, which is UTC and localised by account language:
/// `16 окт. 2026 г., 19:34:47` or `Oct 16, 2026, 7:34:47 PM`.
pub fn parse_strava_csv_date(s: &str) -> Option<i64> {
    let clean: String = s
        .chars()
        .map(|c| if c.is_whitespace() || c == '\u{202f}' || c == '\u{a0}' { ' ' } else { c })
        .collect();
    let words: Vec<&str> = clean.split(|c: char| c == ' ' || c == ',').filter(|w| !w.is_empty()).collect();
    let mut day = None;
    let mut month = None;
    let mut year = None;
    let mut hms: Option<(i64, i64, i64)> = None;
    let mut pm = None;
    for w in &words {
        if w.contains(':') {
            let p: Vec<i64> = w.split(':').filter_map(|x| x.parse().ok()).collect();
            if p.len() >= 2 {
                hms = Some((p[0], p[1], *p.get(2).unwrap_or(&0)));
            }
        } else if let Ok(n) = w.parse::<i64>() {
            if n > 31 {
                year = Some(n);
            } else if day.is_none() {
                day = Some(n);
            }
        } else if w.eq_ignore_ascii_case("pm") {
            pm = Some(true);
        } else if w.eq_ignore_ascii_case("am") {
            pm = Some(false);
        } else if month.is_none() {
            month = month_of(w);
        }
    }
    let (mut h, mi, se) = hms?;
    match pm {
        Some(true) if h < 12 => h += 12,
        Some(false) if h == 12 => h = 0,
        _ => {}
    }
    Some(unix(year?, month?, day?, h, mi, se))
}

/// `YYYY-MM-DD` for a Unix time shifted by `offset_s` (local day).
pub fn ymd(t: i64, offset_s: i64) -> String {
    let days = (t + offset_s).div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// ISO 8601 UTC string for a Unix time.
pub fn iso(t: i64) -> String {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}
