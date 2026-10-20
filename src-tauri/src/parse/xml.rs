//! TCX and GPX readers. A small tag scanner is enough for these flat formats
//! and avoids pulling in an XML library.

use super::{time::parse_iso, Parsed, Sample, Streams};

/// Finds `<prefix:name ...>` or `<name ...>` opening tags; returns byte offsets
/// of (tag start, content start).
fn find_open(s: &str, name: &str, from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while let Some(rel) = s[i..].find('<') {
        let at = i + rel;
        let rest = &s[at + 1..];
        if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('!') {
            i = at + 1;
            continue;
        }
        let end_name = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(rest.len());
        let full = &rest[..end_name];
        let local = full.rsplit(':').next().unwrap_or(full);
        if local == name {
            let close = rest.find('>')?;
            return Some((at, at + 1 + close + 1));
        }
        i = at + 1;
    }
    None
}

fn find_close(s: &str, name: &str, from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(rel) = s[i..].find("</") {
        let at = i + rel;
        let rest = &s[at + 2..];
        let end_name = rest.find('>').unwrap_or(rest.len());
        let full = &rest[..end_name];
        let local = full.rsplit(':').next().unwrap_or(full);
        if local == name {
            return Some(at);
        }
        i = at + 2;
    }
    None
}

/// Inner text of the first `<name>` element in `s`.
fn tag<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let (start, content) = find_open(s, name, 0)?;
    if s[start..content].ends_with("/>") {
        return None;
    }
    let end = find_close(s, name, content)?;
    Some(s[content..end].trim())
}

fn tag_num(s: &str, name: &str) -> Option<f64> {
    tag(s, name).and_then(|v| v.trim().parse::<f64>().ok())
}

/// Every `<name>…</name>` block in `s`.
fn blocks<'a>(s: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((start, content)) = find_open(s, name, i) {
        if s[start..content].ends_with("/>") {
            out.push(&s[start..content]);
            i = content;
            continue;
        }
        match find_close(s, name, content) {
            Some(end) => {
                out.push(&s[start..end]);
                i = end + 2;
            }
            None => break,
        }
    }
    out
}

fn attr(open_tag: &str, name: &str) -> Option<f64> {
    let key = format!("{name}=");
    let at = open_tag.find(&key)? + key.len();
    let q = open_tag[at..].chars().next()?;
    let rest = &open_tag[at + 1..];
    let end = rest.find(q)?;
    rest[..end].parse().ok()
}

fn sport_word(s: &str) -> &'static str {
    match s.trim().to_lowercase().as_str() {
        "running" | "run" | "trail_running" => "run",
        "biking" | "cycling" | "ride" => "ride",
        "walking" | "walk" => "walk",
        "hiking" | "hike" => "hike",
        "swimming" => "swim",
        _ => "workout",
    }
}

fn finish(mut p: Parsed, pts: Vec<(i64, Sample)>) -> Result<Parsed, String> {
    if pts.is_empty() && p.start_utc == 0 {
        return Err("в файле нет точек с временем".into());
    }
    if p.start_utc == 0 {
        p.start_utc = pts[0].0;
    }
    let mut st = Streams::default();
    for (t, s) in pts {
        st.push(t - p.start_utc, s);
    }
    p.streams = st;
    p.fill_from_streams();
    Ok(p)
}

pub fn parse_tcx(text: &str) -> Result<Parsed, String> {
    let act = blocks(text, "Activity").into_iter().next().ok_or("в TCX нет тренировки")?;
    let mut p = Parsed::default();
    let open_end = act.find('>').unwrap_or(0);
    let open = &act[..open_end];
    let sport = open.find("Sport=").map(|i| {
        let r = &open[i + 7..];
        r[..r.find('"').unwrap_or(r.len())].to_string()
    });
    p.sport = sport_word(sport.as_deref().unwrap_or("")).to_string();
    p.sport_label = sport;
    p.start_utc = tag(act, "Id").and_then(parse_iso).unwrap_or(0);
    let mut elapsed = 0.0;
    let mut dist = 0.0;
    let mut cal = 0.0;
    let mut pts = Vec::new();
    for lap in blocks(act, "Lap") {
        elapsed += tag_num(lap, "TotalTimeSeconds").unwrap_or(0.0);
        // Lap-level DistanceMeters appears before the Track; read it from the part before <Track>.
        let head = &lap[..lap.find("<Track").unwrap_or(lap.len())];
        dist += tag_num(head, "DistanceMeters").unwrap_or(0.0);
        cal += tag_num(head, "Calories").unwrap_or(0.0);
        for tp in blocks(lap, "Trackpoint") {
            let Some(t) = tag(tp, "Time").and_then(parse_iso) else { continue };
            let hr = find_open(tp, "HeartRateBpm", 0).and_then(|(_, c)| tag_num(&tp[c..], "Value"));
            pts.push((
                t,
                Sample {
                    hr: hr.filter(|h| *h > 0.0),
                    dist: tag_num(tp, "DistanceMeters"),
                    speed: tag_num(tp, "Speed"),
                    alt: tag_num(tp, "AltitudeMeters"),
                    cad: tag_num(tp, "Cadence").or_else(|| tag_num(tp, "RunCadence")),
                    pwr: tag_num(tp, "Watts"),
                    lat: tag_num(tp, "LatitudeDegrees"),
                    lng: tag_num(tp, "LongitudeDegrees"),
                },
            ));
        }
    }
    if elapsed > 0.0 {
        p.elapsed_s = Some(elapsed);
    }
    if dist > 0.0 {
        p.distance_m = Some(dist);
    }
    if cal > 0.0 {
        p.calories = Some(cal);
    }
    p.device = tag(act, "Creator").and_then(|c| tag(c, "Name")).map(|s| s.to_string());
    finish(p, pts)
}

pub fn parse_gpx(text: &str) -> Result<Parsed, String> {
    let mut p = Parsed::default();
    let trk = blocks(text, "trk").into_iter().next().ok_or("в GPX нет трека")?;
    let head = &trk[..trk.find("<trkseg").unwrap_or(trk.len())];
    p.name = tag(head, "name").map(|s| s.to_string());
    let kind = tag(head, "type").map(|s| s.to_string());
    p.sport = sport_word(kind.as_deref().unwrap_or("")).to_string();
    p.sport_label = kind;
    let mut pts = Vec::new();
    for pt in blocks(trk, "trkpt") {
        let open = &pt[..pt.find('>').unwrap_or(pt.len())];
        let Some(t) = tag(pt, "time").and_then(parse_iso) else { continue };
        pts.push((
            t,
            Sample {
                hr: tag_num(pt, "hr").filter(|h| *h > 0.0),
                dist: None,
                speed: tag_num(pt, "speed"),
                alt: tag_num(pt, "ele"),
                cad: tag_num(pt, "cad"),
                pwr: tag_num(pt, "power"),
                lat: attr(open, "lat"),
                lng: attr(open, "lon"),
            },
        ));
    }
    // GPX has no distance: integrate it from coordinates.
    let mut total = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for (_, s) in pts.iter_mut() {
        if let (Some(la), Some(lo)) = (s.lat, s.lng) {
            if let Some((pla, plo)) = prev {
                total += haversine(pla, plo, la, lo);
            }
            prev = Some((la, lo));
        }
        s.dist = Some(total);
    }
    finish(p, pts)
}

pub fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}
