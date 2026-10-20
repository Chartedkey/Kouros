//! Health tab: last night in detail (hypnogram, need, plan for tonight), HRV and
//! resting pulse against the personal norm, early signals, VO₂max, body,
//! alcohol and caffeine, the habit diary and the history for charts.

use crate::db;
use crate::engine::{add_days, day_of, mean, median, sd, Series};
use crate::engine_svc::{self, series};
use crate::parse::time;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// "2026-10-17T01:26:40.679Z" (or with an offset, or plain unix seconds/ms) → unix seconds.
pub fn iso(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(n) = s.parse::<f64>() {
        return Some(if n > 1e11 { (n / 1000.0) as i64 } else { n as i64 });
    }
    let g = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (g(0, 4)?, g(5, 7)?, g(8, 10)?);
    let (h, mi, sec) = (g(11, 13).unwrap_or(0), g(14, 16).unwrap_or(0), g(17, 19).unwrap_or(0));
    let mut t = time::unix(y, mo, d, h, mi, sec);
    // Offset like +03:00 / -05:00 after the seconds (and optional fraction).
    let rest = s.get(19..).unwrap_or("");
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
        let oh: i64 = rest.get(1..3).and_then(|x| x.parse().ok()).unwrap_or(0);
        let om: i64 = rest.get(4..6).and_then(|x| x.parse().ok()).unwrap_or(0);
        let off = oh * 3600 + om * 60;
        t -= if sign == '+' { off } else { -off };
    }
    Some(t)
}

/// Minutes since local midnight.
fn clock_min(t: i64, tz: i64) -> i64 {
    (t + tz).rem_euclid(86400) / 60
}

#[derive(Clone)]
pub(crate) struct Hyp {
    pub date: String,
    pub segs: Vec<(String, i64, i64)>,
    pub start: i64,
    pub end: i64,
}

impl Hyp {
    /// Asleep at `t`: inside the night and not in an awake stretch.
    pub fn asleep(&self, t: i64) -> bool {
        t >= self.start && t <= self.end && !self.segs.iter().any(|s| s.0 == "awake" && t >= s.1 && t <= s.2)
    }
}

/// All samples of a raw bundle metric (`[{t, v}]` or `{points: [...]}`), merged across rows and
/// device names — one value per timestamp, so duplicates from two device names count once.
pub(crate) fn raw_samples(conn: &Connection, metric: &str, from: &str, lo: f64, hi: f64) -> BTreeMap<i64, f64> {
    let mut out: BTreeMap<i64, f64> = BTreeMap::new();
    let Ok(mut st) = conn.prepare("SELECT txt FROM hm WHERE metric = ?1 AND date >= ?2 AND txt IS NOT NULL") else {
        return out;
    };
    let rows: Vec<String> = st.query_map(params![metric, from], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
    for txt in rows {
        let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
        let arr = v.as_array().cloned().or_else(|| v["points"].as_array().cloned()).unwrap_or_default();
        for x in arr {
            let t = x["t"].as_str().or(x["ts"].as_str()).or(x["time"].as_str()).and_then(iso);
            let val = x["v"].as_f64().or(x["value"].as_f64());
            if let (Some(t), Some(val)) = (t, val) {
                if val >= lo && val <= hi {
                    out.insert(t, val);
                }
            }
        }
    }
    out
}

pub(crate) fn hypnograms(conn: &Connection, from: &str) -> Vec<Hyp> {
    let Ok(mut st) = conn.prepare(
        "SELECT date, txt FROM hm WHERE metric = 'sleep_analysis_raw' AND txt IS NOT NULL AND date >= ?1 ORDER BY date",
    ) else {
        return Vec::new();
    };
    let rows: Vec<(String, String)> = st
        .query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    let mut best: BTreeMap<String, Hyp> = BTreeMap::new();
    for (date, txt) in rows {
        let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
        let arr = v.as_array().cloned().or_else(|| v["stages"].as_array().cloned()).or_else(|| v["samples"].as_array().cloned()).unwrap_or_default();
        let mut segs: Vec<(String, i64, i64)> = arr
            .iter()
            .filter_map(|x| {
                let stage = x["stage"].as_str().or(x["value"].as_str())?.to_lowercase();
                let a = iso(x["start"].as_str().or(x["startDate"].as_str())?)?;
                let b = iso(x["end"].as_str().or(x["endDate"].as_str())?)?;
                (b > a).then_some((stage, a, b))
            })
            .collect();
        if segs.is_empty() {
            continue;
        }
        segs.sort_by_key(|s| s.1);
        let start = segs.first().map(|s| s.1).unwrap_or(0);
        let end = segs.iter().map(|s| s.2).max().unwrap_or(0);
        let h = Hyp { date: date.clone(), segs, start, end };
        // Several records a day = night + naps; the night is the longest.
        if best.get(&date).map(|b| b.end - b.start < end - start).unwrap_or(true) {
            best.insert(date, h);
        }
    }
    best.into_values().collect()
}

/// Night HRV: median of the watch's HRV samples (SDNN) taken while asleep — awake stretches
/// excluded, duplicates from two device names merged. Needs at least 5 samples a night.
/// Daytime readings are lower and would drag a whole-day average down as the day goes on.
pub fn nightly_hrv(conn: &Connection) -> Series {
    let mut out = Series::new();
    let hyps = hypnograms(conn, "2000-01-01");
    if hyps.is_empty() {
        return out;
    }
    let samples = raw_samples(conn, "hrv_sdnn_raw", "2000-01-01", 5.0, 300.0);
    for h in hyps {
        let v: Vec<f64> = samples.range(h.start..=h.end).filter(|(t, _)| h.asleep(**t)).map(|(_, v)| *v).collect();
        if v.len() >= 5 {
            if let Some(m) = median(&v) {
                out.insert(h.date.clone(), r1(m));
            }
        }
    }
    out
}

/// Night resting pulse: the calmest 30 minutes of sleep (lowest average over a 30-minute window,
/// at least 3 samples in it), awake stretches excluded. A single low reading can be a sensor
/// glitch, so one sample never decides. Needs at least 10 samples a night.
pub fn nightly_rhr(conn: &Connection) -> Series {
    let mut out = Series::new();
    let hyps = hypnograms(conn, "2000-01-01");
    if hyps.is_empty() {
        return out;
    }
    let samples = raw_samples(conn, "heart_rate_raw", "2000-01-01", 25.0, 140.0);
    for h in hyps {
        let v: Vec<(i64, f64)> = samples.range(h.start..=h.end).filter(|(t, _)| h.asleep(**t)).map(|(t, v)| (*t, *v)).collect();
        if v.len() < 10 {
            continue;
        }
        let mut best: Option<f64> = None;
        let mut j = 0;
        for i in 0..v.len() {
            if j < i {
                j = i;
            }
            while j < v.len() && v[j].0 <= v[i].0 + 1800 {
                j += 1;
            }
            let w = &v[i..j];
            if w.len() >= 3 {
                let m = w.iter().map(|x| x.1).sum::<f64>() / w.len() as f64;
                if best.map(|b| m < b).unwrap_or(true) {
                    best = Some(m);
                }
            }
        }
        if let Some(b) = best {
            out.insert(h.date.clone(), b.round());
        }
    }
    out
}

/// Night SpO₂: median of the watch's background readings while asleep (awake excluded), at least 3
/// a night. Daytime readings are noisier (movement, hand position).
pub fn nightly_spo2(conn: &Connection) -> Series {
    let mut out = Series::new();
    let hyps = hypnograms(conn, "2000-01-01");
    if hyps.is_empty() {
        return out;
    }
    let mut samples: BTreeMap<i64, f64> = BTreeMap::new();
    if let Ok(mut st) = conn.prepare("SELECT ts, num FROM hm WHERE metric = 'blood_oxygen' AND num IS NOT NULL") {
        let rows: Vec<(String, f64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
        for (ts, v) in rows {
            let v = if v <= 1.0 { v * 100.0 } else { v };
            if let Some(t) = iso(&ts) {
                if (70.0..=100.0).contains(&v) {
                    samples.insert(t, v);
                }
            }
        }
    }
    for h in hyps {
        let v: Vec<f64> = samples.range(h.start..=h.end).filter(|(t, _)| h.asleep(**t)).map(|(_, v)| *v).collect();
        if v.len() >= 3 {
            if let Some(m) = median(&v) {
                out.insert(h.date.clone(), r1(m));
            }
        }
    }
    out
}

/// SpO₂ every screen uses: night values when there are at least 4 nights, else the daily average.
pub fn spo2_series(conn: &Connection) -> (Series, bool) {
    let d = series(conn, "blood_oxygen_avg", "avg");
    let d = d.iter().map(|(k, v)| (k.clone(), if *v <= 1.0 { v * 100.0 } else { *v })).collect();
    by_source(conn, "vitals", nightly_spo2(conn), d)
}

/// Night values (from Apple's samples) or the daily figure. When another source was chosen for the
/// group, its daily figure is used from the switch on (the whole history if it replaced it).
fn by_source(conn: &Connection, group: &str, night: Series, daily: Series) -> (Series, bool) {
    let pick = |night: Series, daily: Series| if night.len() >= 4 { (night, true) } else { (daily, false) };
    if crate::sources::current_is_apple(conn, group) {
        return pick(night, daily);
    }
    match crate::sources::cut(conn, group) {
        None => (daily, false),
        Some(c) => {
            let (old, _) = pick(night.into_iter().filter(|(d, _)| *d < c).collect(), daily.iter().filter(|(d, _)| **d < c).map(|(a, b)| (a.clone(), *b)).collect());
            let mut out = old;
            out.extend(daily.into_iter().filter(|(d, _)| *d >= c));
            (out, false)
        }
    }
}

/// Resting pulse every screen uses: night values when there are enough, otherwise Apple's daily
/// figure. Never a mix — the night value is lower.
pub fn rhr_series(conn: &Connection) -> (Series, bool) {
    by_source(conn, "rhr", nightly_rhr(conn), series(conn, "resting_heart_rate_merged_avg", "avg"))
}

/// The HRV series every screen uses: night values when there are enough of them, otherwise the
/// daily average from Apple Health. Never a mix of the two — they are on different levels.
pub fn hrv_series(conn: &Connection) -> (Series, bool) {
    let a = series(conn, "hrv_rmssd_merged_avg", "avg");
    let d = if a.is_empty() { series(conn, "hrv_rmssd_avg", "avg") } else { a };
    let d = if d.is_empty() { series(conn, "hrv_sdnn_merged_avg", "avg") } else { d };
    by_source(conn, "hrv", nightly_hrv(conn), d)
}

/// Sleep need model (0.17): your own need from the nights after which HRV and resting pulse were
/// in shape (7–9 h), a tolerance band so small swings are not debt, and a chronic short-sleep flag.
pub struct NeedModel {
    pub need: f64,
    pub band: f64,
    pub personal: Option<f64>,
    pub nights: usize,
    pub manual: bool,
    pub chronic: Option<String>,
}

pub fn need_model(totals: &Series, hrv: &Series, rhr: &Series, today: &str, saved: f64, manual: bool) -> NeedModel {
    let from = add_days(today, -60);
    let mut good: Vec<f64> = Vec::new();
    for (d, t) in totals.range(from.clone()..add_days(today, 1)) {
        let ok_h = hrv.get(d).and_then(|v| crate::engine::daily::corridor(&crate::engine::window(hrv, d, -30, 0), 6.0).map(|(m, lo, _)| *v >= lo && *v >= m - 0.25 * (m - lo)));
        let ok_r = rhr.get(d).and_then(|v| crate::engine::daily::corridor(&crate::engine::window(rhr, d, -30, 0), 2.0).map(|(m, _, hi)| *v <= hi && *v <= m + 0.25 * (hi - m)));
        if ok_h.unwrap_or(false) && ok_r.unwrap_or(true) {
            good.push(t / 60.0);
        }
    }
    let personal = if good.len() >= 10 { median(&good).map(|m| ((m / 5.0).round() * 5.0).clamp(420.0, 540.0)) } else { None };
    let need = if manual { saved } else { personal.unwrap_or(saved) };
    let last30: Vec<f64> = totals.range(add_days(today, -29)..add_days(today, 1)).map(|(_, t)| t / 60.0).collect();
    let band = (sd(&last30).unwrap_or(40.0) * 0.5).clamp(15.0, 30.0).round();
    let last14: Vec<f64> = totals.range(add_days(today, -13)..add_days(today, 1)).map(|(_, t)| t / 60.0).collect();
    let chronic = if last14.len() >= 10 {
        let avg = last14.iter().sum::<f64>() / last14.len() as f64;
        let short = last14.iter().filter(|t| **t < need - band).count();
        if avg < 420.0 {
            Some(format!("За 2 недели сон в среднем {} — меньше 7 часов. Это хронический недосып: восстановление и иммунитет страдают, даже если кажется, что привык.", hm_words(avg)))
        } else if short >= 10 {
            Some(format!("{short} ночей из {} ниже твоей потребности ({}) — недосып накапливается.", last14.len(), hm_words(need)))
        } else {
            None
        }
    } else {
        None
    };
    NeedModel { need, band, personal, nights: good.len(), manual, chronic }
}

/// Debt over the last 7 nights: only what is below the band; a long night pays half of its extra back.
pub fn debt_of(totals: &Series, today: &str, need: f64, band: f64) -> f64 {
    let mut debt: f64 = 0.0;
    for k in (0..7).rev() {
        let Some(t) = totals.get(&add_days(today, -k)).map(|t| t / 60.0) else { continue };
        if t < need - band {
            debt += need - band - t;
        } else if t > need + band {
            debt = (debt - 0.5 * (t - need - band)).max(0.0);
        }
    }
    debt
}

/// The same model straight from settings (for the engine summary).
pub fn need_for(conn: &Connection, totals: &Series, hrv: &Series, rhr: &Series, today: &str) -> NeedModel {
    let sched = db::get_setting(conn, "sleep_schedule").and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
    let base = sched["baseMin"].as_f64().unwrap_or(465.0).clamp(360.0, 600.0);
    need_model(totals, hrv, rhr, today, base, sched["needMode"].as_str() == Some("manual"))
}

/// Personal norm: median ± max(k·sd, floor) over the window before `today`.
fn norm(s: &Series, today: &str, days: i64, k: f64, floor: f64) -> Option<(f64, f64, f64)> {
    let v = crate::engine::window(s, today, -days, 0);
    if v.len() < 4 {
        return None;
    }
    let m = median(&v)?;
    let w = (sd(&v).unwrap_or(0.0) * k).max(floor);
    Some((m, m - w, m + w))
}

fn get_json(conn: &Connection, key: &str, def: Value) -> Value {
    db::get_setting(conn, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(def)
}

fn hm_words(min: f64) -> String {
    let m = min.round() as i64;
    if m >= 60 { format!("{} ч {} мин", m / 60, m % 60) } else { format!("{m} мин") }
}

fn hhmm(m: i64) -> String {
    let m = m.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

fn parse_hhmm(s: &str) -> Option<i64> {
    let (h, m) = s.split_once(':')?;
    Some(h.trim().parse::<i64>().ok()? * 60 + m.trim().parse::<i64>().ok()?)
}

pub fn summary(conn: &Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let tz = p.tz_offset_s;
    let today = engine_svc::today(&p);
    let from90 = add_days(&today, -90);

    // ----- series -----
    let stress_all = crate::stress_svc::days(conn, &p, &from90, &today);
    let stress_v = json!({
        "series": stress_all.iter().filter_map(|(d, s)| s.score.map(|v| json!([d, v]))).collect::<Vec<_>>(),
        "today": stress_all.get(&today).map(|s| crate::stress_svc::day_json(s, tz, crate::stress_svc::norm(&stress_all, &today))),
        "norm": crate::stress_svc::norm(&stress_all, &add_days(&today, 1)),
    });
    let (hrv, hrv_nightly) = hrv_series(conn);
    let (rhr, rhr_nightly) = rhr_series(conn);
    let resp = series(conn, "respiratory_rate_merged_avg", "avg");
    let temp = series(conn, "wrist_temp", "avg");
    let (spo2, _) = spo2_series(conn);
    let core = series(conn, "sleep_analysis_core_seconds", "max");
    let deep = series(conn, "sleep_analysis_deep_seconds", "max");
    let rem = series(conn, "sleep_analysis_rem_seconds", "max");
    let awake = series(conn, "sleep_analysis_awake_seconds", "max");
    let vo2 = series(conn, "vo2_max", "median");
    let steps = series(conn, "steps_total", "max");
    let weight = series(conn, "body_mass", "median");
    let fat = series(conn, "body_fat_pct", "median");
    let lean = series(conn, "lean_body_mass", "median");
    let alcohol = series(conn, "alcoholic_beverages_total", "max");
    let caffeine = series(conn, "dietary_caffeine_total", "max");

    // ----- nights -----
    let hyps = hypnograms(conn, &add_days(&today, -100));
    let hyp_by: BTreeMap<String, Hyp> = hyps.iter().map(|h| (h.date.clone(), h.clone())).collect();
    let mut nights: Vec<Value> = Vec::new();
    let mut totals: Series = Series::new();
    for d in core.keys().chain(deep.keys()).filter(|d| **d >= from90).collect::<std::collections::BTreeSet<_>>() {
        let Some(n) = crate::engine::daily::night(d, &core, &deep, &rem, &awake) else { continue };
        totals.insert(d.clone(), n.total_s);
        let h = hyp_by.get(d);
        nights.push(json!({
            "date": d, "totalS": n.total_s.round(), "deepS": n.deep_s.round(), "coreS": n.core_s.round(), "remS": n.rem_s.round(),
            "awakeS": n.awake_s.round(), "score": n.score,
            "bedUtc": h.map(|h| h.start), "wakeUtc": h.map(|h| h.end),
        }));
    }
    let last = nights.last().cloned();

    // ----- need for tonight -----
    // Habitual rhythm from the watch: median bedtime and wake time over 21 nights.
    let recent: Vec<&Hyp> = hyps.iter().filter(|h| h.date > add_days(&today, -21)).collect();
    let rhythm = if recent.len() >= 5 {
        let bm: Vec<f64> = recent
            .iter()
            .map(|h| {
                let m = clock_min(h.start, tz) as f64;
                if m < 15.0 * 60.0 { m + 1440.0 } else { m }
            })
            .collect();
        let wm: Vec<f64> = recent.iter().map(|h| clock_min(h.end, tz) as f64).collect();
        match (median(&bm), median(&wm)) {
            (Some(b), Some(w)) => Some(((b.round() as i64 / 15) * 15, (w.round() as i64 / 15) * 15)),
            _ => None,
        }
    } else {
        None
    };
    let saved = db::get_setting(conn, "sleep_schedule").and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let sched = match (&saved, rhythm) {
        (Some(v), _) => v.clone(),
        (None, Some((_, w))) => json!({"weekday": hhmm(w), "weekend": hhmm(w), "baseMin": 465, "remind": true}),
        _ => json!({"weekday": "07:30", "weekend": "08:30", "baseMin": 465, "remind": true}),
    };
    let sched_source = if saved.is_some() { "user" } else if rhythm.is_some() { "watch" } else { "default" };
    let saved_base = sched["baseMin"].as_f64().unwrap_or(465.0).clamp(360.0, 600.0);
    let nm = need_model(&totals, &hrv, &rhr, &today, saved_base, sched["needMode"].as_str() == Some("manual"));
    let base = nm.need;
    let debt_min: f64 = debt_of(&totals, &today, nm.need, nm.band);
    let (load_today, usual): (f64, f64) = {
        let since = time::unix(
            today[0..4].parse().unwrap_or(1970),
            today[5..7].parse().unwrap_or(1),
            today[8..10].parse().unwrap_or(1),
            0,
            0,
            0,
        ) - tz;
        let lt: f64 = conn
            .query_row(
                "SELECT COALESCE(sum(m.load), 0) FROM va a JOIN activity_metrics m ON m.activity_id = a.id WHERE a.start_utc >= ?1",
                params![since],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        let us: f64 = conn
            .query_row(
                "SELECT COALESCE(sum(m.load), 0) / 42.0 FROM va a JOIN activity_metrics m ON m.activity_id = a.id WHERE a.start_utc >= ?1 AND a.start_utc < ?2",
                params![since - 42 * 86400, since],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        (lt, us)
    };
    let load_add = if usual > 1.0 { ((load_today / usual - 1.0) * 30.0).clamp(0.0, 30.0) } else { 0.0 };
    let repay = (debt_min / 4.0).min(45.0);
    let nap_min: f64 = {
        let st = conn.prepare(
            "SELECT txt FROM hm WHERE metric = 'sleep_analysis_raw' AND txt IS NOT NULL AND date = ?1",
        );
        let mut total = 0.0;
        if let Ok(mut st) = st {
            let rows: Vec<String> = st.query_map(params![today], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
            let night_end = hyp_by.get(&today).map(|h| h.end).unwrap_or(0);
            for txt in rows {
                if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&txt) {
                    let s = a.first().and_then(|x| x["start"].as_str()).and_then(iso).unwrap_or(0);
                    if s > night_end + 3600 {
                        let e = a.last().and_then(|x| x["end"].as_str()).and_then(iso).unwrap_or(s);
                        total += ((e - s) as f64 / 60.0).max(0.0);
                    }
                }
            }
        }
        total
    };

    // ----- early signals -----
    struct Sig {
        key: &'static str,
        name: &'static str,
        unit: &'static str,
    }
    let sigs = [
        Sig { key: "resp", name: "Частота дыхания", unit: "/мин" },
        Sig { key: "temp", name: "Температура запястья", unit: "°C" },
        Sig { key: "rhr", name: "Пульс покоя", unit: "уд/мин" },
        Sig { key: "spo2", name: "Кислород в крови", unit: "%" },
        Sig { key: "hrv", name: "HRV", unit: "мс" },
    ];
    let status = |key: &str, d: &str| -> (Option<f64>, Option<(f64, f64, f64)>, i64) {
        let (s, k, floor): (&Series, f64, f64) = match key {
            "resp" => (&resp, 1.5, 0.5),
            "temp" => (&temp, 1.0, 0.5),
            "rhr" => (&rhr, 1.0, 3.0),
            "spo2" => (&spo2, 1.5, 2.0),
            _ => (&hrv, 1.0, 5.0),
        };
        let v = s.get(d).copied();
        let n = norm(s, d, 30, k, floor);
        let lvl = match (v, n) {
            (Some(v), Some((m, lo, hi))) => {
                let w = (hi - m).max(1e-6);
                // How far outside the norm, in norm widths; only the harmful side counts.
                let out = match key {
                    "spo2" | "hrv" => lo - v,
                    "temp" => (v - m).abs() - w,
                    _ => v - hi,
                };
                if out <= 0.0 { 0 } else if out < w { 1 } else { 2 }
            }
            _ => -1,
        };
        (v, n, lvl)
    };
    let today_sig: Vec<Value> = sigs
        .iter()
        .filter(|s| s.key != "hrv")
        .map(|s| {
            // Overnight values may land on yesterday's date until the day fills in.
            let (mut v, mut n, mut l) = status(s.key, &today);
            let mut date = today.clone();
            if v.is_none() {
                let y = add_days(&today, -1);
                (v, n, l) = status(s.key, &y);
                date = y;
            }
            // Wrist temperature is shown as the shift from your own baseline.
            let (v, n) = if s.key == "temp" {
                match n {
                    Some((m, lo, hi)) => (v.map(|x| ((x - m) * 100.0).round() / 100.0), Some((0.0, lo - m, hi - m))),
                    None => (None, None),
                }
            } else {
                (v.map(r1), n)
            };
            json!({"key": s.key, "name": s.name, "unit": s.unit, "value": v, "date": date,
                   "norm": n.map(|(m, lo, hi)| json!([r1(m), r1(lo), r1(hi)])), "level": l})
        })
        .collect();
    let strong = today_sig.iter().filter(|s| s["level"].as_i64() == Some(2)).count();
    let ill_add = if strong >= 2 { 30.0 } else { 0.0 };
    let need = base + load_add + repay - nap_min + ill_add;

    // Which wake-up comes next: today's if this night's sleep is not recorded yet.
    let now = crate::store::now();
    let slept_today = totals.get(&today).map(|t| *t >= 3.0 * 3600.0).unwrap_or(false);
    let midnight = time::unix(today[0..4].parse().unwrap_or(1970), today[5..7].parse().unwrap_or(1), today[8..10].parse().unwrap_or(1), 0, 0, 0) - tz;
    let wake_for = |d: &str| -> i64 {
        let wd = (crate::engine::day_num(d).unwrap_or(0) + 3).rem_euclid(7); // 0 = Monday
        parse_hhmm(sched[if wd >= 5 { "weekend" } else { "weekday" }].as_str().unwrap_or("07:30")).unwrap_or(450)
    };
    let wake_today = midnight + wake_for(&today) * 60;
    let wake_date = if !slept_today && now < wake_today + 3 * 3600 { today.clone() } else { add_days(&today, 1) };
    let weekend = (crate::engine::day_num(&wake_date).unwrap_or(0) + 3).rem_euclid(7) >= 5;
    let mut wake = wake_for(&wake_date);
    // Habitual plan: the usual amount of sleep before the usual wake-up.
    let mut bed = wake - base.round() as i64 - 15;

    // Suggestion for tonight: sleep a bit more when there is a reason.
    let mut reasons: Vec<String> = Vec::new();
    if debt_min > 120.0 {
        reasons.push(format!("долг сна за неделю {}", hm_words(debt_min)));
    }
    let two = |key: &str| (0..2).all(|k| status(key, &add_days(&today, -k)).2 >= 1);
    if two("hrv") {
        reasons.push("HRV ниже нормы второе утро подряд".into());
    }
    if two("rhr") {
        reasons.push("пульс покоя выше нормы второе утро подряд".into());
    }
    if alcohol.get(&today).or(alcohol.get(&add_days(&today, -1))).map(|v| *v > 0.0).unwrap_or(false) {
        reasons.push("был алкоголь".into());
    }
    if strong >= 2 {
        reasons.push("несколько ранних сигналов болезни".into());
    }
    if usual > 1.0 && load_today > usual * 1.5 {
        reasons.push("тяжёлая нагрузка сегодня".into());
    }
    let extra = (load_add + repay + ill_add - nap_min).max(if reasons.is_empty() { 0.0 } else { 30.0 });
    let shift = (((extra / 15.0).round() as i64) * 15).clamp(0, 45);
    let accepted = get_json(conn, "sleep_tonight", Value::Null);
    let accepted = (accepted["date"].as_str() == Some(wake_date.as_str())).then(|| accepted.clone());
    if let Some(a) = &accepted {
        let m = a["min"].as_i64().unwrap_or(0);
        match a["mode"].as_str() {
            Some("earlier") => bed -= m,
            Some("later") => wake += m,
            _ => {}
        }
    }
    let lights = bed - 30;
    let suggestion = (shift >= 15 && accepted.is_none()).then(|| json!({
        "min": shift, "reasons": reasons, "date": wake_date,
        "earlierBed": hhmm(bed - shift), "laterWake": hhmm(wake + shift),
    }));
    // Night state for the Today tab.
    let wake_abs = time::unix(wake_date[0..4].parse().unwrap_or(1970), wake_date[5..7].parse().unwrap_or(1), wake_date[8..10].parse().unwrap_or(1), 0, 0, 0) - tz + wake * 60;
    let span = (wake - bed).rem_euclid(1440);
    let bed_abs = wake_abs - span * 60;
    let phase = if slept_today && now < bed_abs - 90 * 60 {
        "day"
    } else if now < bed_abs - 90 * 60 {
        if !slept_today { "before_sleep" } else { "day" }
    } else if now < bed_abs - 30 * 60 {
        "evening"
    } else if now < bed_abs {
        "lights"
    } else if now < wake_abs + 2 * 3600 {
        "sleep"
    } else {
        "day"
    };
    let night_state = json!({"phase": phase, "now": now, "bedAbs": bed_abs, "wakeAbs": wake_abs, "minToBed": (bed_abs - now) / 60,
        "sleptToday": slept_today, "useYesterday": !slept_today && now < wake_today + 3 * 3600, "debtMin": debt_min.round()});
    let caffeine_cut = bed - 400; // ≈ 6 h 40 min before bed

    let hist_days: Vec<String> = (0..14).rev().map(|k| add_days(&today, -k)).collect();
    let sig_matrix: Vec<Value> = sigs
        .iter()
        .map(|s| {
            json!({"key": s.key, "name": s.name, "unit": s.unit, "days": hist_days.iter().map(|d| {
                let (v, _, l) = status(s.key, d);
                json!([d, v.map(r1), l])
            }).collect::<Vec<_>>()})
        })
        .collect();

    // ----- HRV / resting pulse -----
    let ser30 = |s: &Series| -> Vec<Value> { s.range(add_days(&today, -29)..add_days(&today, 1)).map(|(d, v)| json!([d, r1(*v)])).collect() };
    let ser90 = |s: &Series| -> Vec<Value> { s.range(from90.clone()..add_days(&today, 1)).map(|(d, v)| json!([d, r1(*v)])).collect() };
    let cuts = crate::sources::cuts(conn);
    let band_g = |g: &str, s: &Series, k: f64, floor: f64| {
        // After a switch of source the norm is built only from the new source's days.
        let trimmed: Series;
        let s = match cuts.get(g) {
            Some(c) => {
                trimmed = s.range(c.clone()..).map(|(a, b)| (a.clone(), *b)).collect();
                &trimmed
            }
            None => s,
        };
        norm(s, &add_days(&today, 1), 30, k, floor).map(|(m, lo, hi)| json!([r1(m), r1(lo), r1(hi)]))
    };

    // ----- body -----
    let weighings: Vec<Value> = weight
        .range(add_days(&today, -120)..add_days(&today, 1))
        .rev()
        .take(12)
        .map(|(d, w)| json!({"date": d, "weight": r1(*w), "fat": fat.get(d).map(|f| r1(if *f <= 1.0 { f * 100.0 } else { *f })), "lean": lean.get(d).map(|x| r1(*x))}))
        .collect();
    let steps7 = mean(&crate::engine::window(&steps, &today, -6, 1));

    // ----- alcohol -----
    let next_morning = |s: &Series, with: bool| -> Option<f64> {
        let v: Vec<f64> = s
            .iter()
            .filter(|(d, _)| **d >= add_days(&today, -60))
            .filter_map(|(d, v)| {
                let prev = add_days(d, -1);
                let drank = alcohol.get(&prev).map(|a| *a > 0.0).unwrap_or(false);
                (drank == with).then_some(*v)
            })
            .collect();
        if v.len() >= 2 { mean(&v) } else { None }
    };
    let hrv_after = match (next_morning(&hrv, true), next_morning(&hrv, false)) {
        (Some(a), Some(b)) if b > 0.0 => Some(r1((a / b - 1.0) * 100.0)),
        _ => None,
    };
    let rhr_after = match (next_morning(&rhr, true), next_morning(&rhr, false)) {
        (Some(a), Some(b)) => Some(r1(a - b)),
        _ => None,
    };
    let entries = |metric: &str, from: &str| -> Vec<(i64, f64)> {
        let Ok(mut st) = conn.prepare("SELECT ts, num FROM hm WHERE metric = ?1 AND date >= ?2 AND num IS NOT NULL ORDER BY ts") else {
            return Vec::new();
        };
        st.query_map(params![metric, from], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
            .map(|r| r.flatten().filter_map(|(t, v)| iso(&t).map(|t| (t, v))).collect())
            .unwrap_or_default()
    };
    let drinks = entries("drink_entry_alcohol_units", &add_days(&today, -4));
    // The last session: entries no more than 6 h apart, counting back from the latest.
    let mut session: Vec<(i64, f64)> = Vec::new();
    for e in drinks.iter().rev() {
        if session.last().map(|l| l.0 - e.0 <= 6 * 3600).unwrap_or(true) {
            session.push(*e);
        } else {
            break;
        }
    }
    session.reverse();
    let volumes = entries("drink_entry_volume", &add_days(&today, -4));
    let session_v: Vec<Value> = session
        .iter()
        .map(|(t, u)| {
            let vol = volumes.iter().find(|(vt, _)| (vt - t).abs() <= 120).map(|x| x.1);
            json!({"ts": t, "units": u, "volumeMl": vol})
        })
        .collect();
    let alc30: Vec<Value> = (0..30).rev().map(|k| add_days(&today, -k)).map(|d| json!([d.clone(), alcohol.get(&d).copied().unwrap_or(0.0)])).collect();
    let alc_known = alcohol.range(add_days(&today, -29)..add_days(&today, 1)).count();

    // ----- caffeine -----
    let caf_entries = entries("food_entry_caffeine", &add_days(&today, -30));
    let mut caf_last: BTreeMap<String, i64> = BTreeMap::new();
    for (t, _) in &caf_entries {
        let d = day_of(*t, tz);
        let m = clock_min(*t, tz);
        let e = caf_last.entry(d).or_insert(m);
        if m > *e {
            *e = m;
        }
    }
    let caf_vals: Vec<f64> = caffeine.range(add_days(&today, -29)..add_days(&today, 1)).map(|(_, v)| *v).collect();
    let late_days = caf_last.values().filter(|m| **m > caffeine_cut.rem_euclid(1440) && **m > 600).count();
    let caf_days: Vec<Value> = caf_last.iter().rev().take(10).map(|(d, m)| json!([d, hhmm(*m), caffeine.get(d).map(|v| v.round())])).collect();

    // ----- habit diary -----
    let custom: Vec<String> = serde_json::from_str(&db::get_setting(conn, "habits_custom").unwrap_or_else(|| "[]".into())).unwrap_or_default();
    let mut marks: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    {
        let mut st = conn.prepare("SELECT date, key, val FROM habit_log WHERE date >= ?1").map_err(err)?;
        let rows = st.query_map(params![add_days(&today, -120)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map_err(err)?;
        for (d, k, v) in rows.flatten() {
            marks.entry(k).or_default().insert(d, v);
        }
    }
    // Automatic marks from the data.
    let food_late: Vec<String> = entries("food_entry_energy", &add_days(&today, -120))
        .iter()
        .filter(|(t, _)| clock_min(*t, tz) >= 21 * 60)
        .map(|(t, _)| day_of(*t, tz))
        .collect();
    let auto_alc: BTreeMap<String, i64> = alcohol.iter().map(|(d, v)| (d.clone(), (*v > 0.0) as i64)).collect();
    let auto_caf: BTreeMap<String, i64> = caf_last.iter().map(|(d, m)| (d.clone(), (*m > caffeine_cut.rem_euclid(1440) && *m > 600) as i64)).collect();
    let food_days: std::collections::BTreeSet<String> =
        entries("food_entry_energy", &add_days(&today, -120)).iter().map(|(t, _)| day_of(*t, tz)).collect();
    let auto_dinner: BTreeMap<String, i64> = food_days.iter().map(|d| (d.clone(), food_late.contains(d) as i64)).collect();
    let effect = |m: &BTreeMap<String, i64>| -> Value {
        let (mut with, mut without) = (Vec::new(), Vec::new());
        for (d, v) in m {
            if let Some(h) = hrv.get(&add_days(d, 1)) {
                if *v > 0 { with.push(*h) } else { without.push(*h) }
            }
        }
        let pct = if with.len() >= 5 && without.len() >= 5 {
            match (mean(&with), mean(&without)) {
                (Some(a), Some(b)) if b > 0.0 => Some(r1((a / b - 1.0) * 100.0)),
                _ => None,
            }
        } else {
            None
        };
        json!({"with": with.len(), "without": without.len(), "pct": pct})
    };
    let mut habits: Vec<Value> = vec![
        json!({"key": "alcohol", "name": "Алкоголь", "auto": "Waterllama", "today": auto_alc.get(&today), "effect": effect(&auto_alc)}),
        json!({"key": "caffeine_late", "name": format!("Кофеин после {}", hhmm(caffeine_cut)), "auto": "Waterllama", "today": auto_caf.get(&today), "effect": effect(&auto_caf)}),
        json!({"key": "late_dinner", "name": "Поздний ужин (после 21:00)", "auto": "YAZIO", "today": auto_dinner.get(&today), "effect": effect(&auto_dinner)}),
    ];
    let manual: Vec<(String, String)> = [("screens", "Экраны в кровати"), ("sauna", "Сауна или баня"), ("breath", "Медитация, дыхание"), ("stretch", "Растяжка вечером")]
        .iter()
        .map(|(k, n)| (k.to_string(), n.to_string()))
        .chain(custom.iter().map(|n| (format!("c:{n}"), n.clone())))
        .collect();
    for (k, n) in manual {
        let m = marks.get(&k).cloned().unwrap_or_default();
        habits.push(json!({"key": k, "name": n, "auto": null, "today": m.get(&today), "effect": effect(&m)}));
    }

    // ----- sleep regularity and sub-scores -----
    let beds: Vec<f64> = hyps
        .iter()
        .filter(|h| h.date > add_days(&today, -7))
        .map(|h| {
            let m = clock_min(h.start, tz) as f64;
            if m < 12.0 * 60.0 { m + 1440.0 } else { m } // after midnight counts as late evening
        })
        .collect();
    let regularity = sd(&beds).map(|x| x.round());
    let last_hyp = hyp_by.get(&today).or_else(|| hyp_by.get(&add_days(&today, -1)));
    let wakeups = last_hyp.map(|h| {
        let n = h.segs.len();
        let inner: Vec<&(String, i64, i64)> = h.segs.iter().enumerate().filter(|(i, s)| s.0 == "awake" && *i > 0 && *i + 1 < n).map(|(_, s)| s).collect();
        (inner.len(), inner.iter().map(|s| s.2 - s.1).sum::<i64>() / 60)
    });

    Ok(json!({
        "today": today,
        "tz": tz,
        "signals": today_sig,
        "signalMatrix": sig_matrix,
        "night": last,
        "hypnogram": last_hyp.map(|h| json!({"date": h.date, "start": h.start, "end": h.end,
            "segs": h.segs.iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>()})),
        "wakeups": wakeups.map(|(n, m)| json!({"count": n, "min": m})),
        "regularityMin": regularity,
        "nights": nights,
        "need": {"baseMin": base, "savedBaseMin": saved_base, "personalMin": nm.personal, "personalNights": nm.nights, "bandMin": nm.band,
                 "needMode": if nm.manual { "manual" } else { "auto" }, "chronic": nm.chronic, "loadMin": load_add.round(), "repayMin": repay.round(), "napMin": nap_min.round(), "illMin": ill_add,
                 "totalMin": need.round(), "debtMin": debt_min.round()},
        "plan": {"wake": hhmm(wake), "bed": hhmm(bed), "lights": hhmm(lights), "weekend": weekend, "caffeineCut": hhmm(caffeine_cut),
                 "date": wake_date, "accepted": accepted},
        "schedule": sched,
        "scheduleSource": sched_source,
        "rhythm": rhythm.map(|(b, w)| json!({"bed": hhmm(b), "wake": hhmm(w), "nights": recent.len()})),
        "suggestion": suggestion,
        "nightState": night_state,
        "hrv": {"series": ser90(&hrv), "last30": ser30(&hrv), "band": band_g("hrv", &hrv, 0.75, 6.0), "nightly": hrv_nightly},
        "stress": stress_v,
        "rhr": {"series": ser90(&rhr), "last30": ser30(&rhr), "band": band_g("rhr", &rhr, 0.75, 2.0), "nightly": rhr_nightly},
        "vo2": {"series": ser90(&vo2), "lab": get_json(conn, "lab_vo2", Value::Null)},
        "body": {"weighings": weighings, "steps7": steps7.map(|s| s.round()), "steps": ser30(&steps),
                 "weight": weight.iter().map(|(d, v)| json!([d, r1(*v)])).collect::<Vec<_>>(),
                 "fat": fat.iter().map(|(d, v)| json!([d, r1(if *v <= 1.0 { v * 100.0 } else { *v })])).collect::<Vec<_>>(),
                 "lean": lean.iter().map(|(d, v)| json!([d, r1(*v)])).collect::<Vec<_>>()},
        "alcohol": {"days": alc30, "known": alc_known, "hrvAfterPct": hrv_after, "rhrAfter": rhr_after, "session": session_v, "weightKg": weight.values().next_back().copied()},
        "caffeine": {"avg": mean(&caf_vals).map(|x| x.round()), "latest": caf_last.values().max().map(|m| hhmm(*m)), "lateDays": late_days, "days": caf_days, "known": caf_last.len()},
        "habits": habits,
        "switches": crate::sources::switches(conn),
    }))
}

pub fn mark(conn: &Connection, date: &str, key: &str, val: Option<i64>) -> Result<(), String> {
    match val {
        Some(v) => conn
            .execute("INSERT INTO habit_log (date, key, val) VALUES (?1, ?2, ?3) ON CONFLICT(date, key) DO UPDATE SET val = excluded.val", params![date, key, v])
            .map(|_| ())
            .map_err(err),
        None => conn.execute("DELETE FROM habit_log WHERE date = ?1 AND key = ?2", params![date, key]).map(|_| ()).map_err(err),
    }
}

pub fn set(conn: &Connection, key: &str, v: &Value) -> Result<(), String> {
    match key {
        "sleep_schedule" => {
            for k in ["weekday", "weekend"] {
                if v[k].as_str().and_then(parse_hhmm).is_none() {
                    return Err("Время в виде 07:30".into());
                }
            }
        }
        "habits_custom" if !v.is_array() => return Err("Ожидался список".into()),
        "habits_custom" | "lab_vo2" | "sleep_tonight" => {}
        "nutrient_norms" if !v.as_str().map(|c| crate::norms::COUNTRIES.iter().any(|x| x.0 == c)).unwrap_or(false) => return Err("Неизвестная страна".into()),
        "nutrient_norms" => {}
        _ => return Err("Неизвестная настройка".into()),
    }
    db::set_setting(conn, key, &v.to_string())
}

/// Everything known about one day, for the day card in History.
pub fn day(conn: &Connection, date: &str) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let tz = p.tz_offset_s;
    let one = |metric: &str, how: &str| -> Option<f64> {
        let agg = match how {
            "sum" => "sum(num)",
            "max" => "max(num)",
            _ => "avg(num)",
        };
        conn.query_row(
            &format!("SELECT {agg} FROM hm WHERE metric = ?1 AND date = ?2 AND num IS NOT NULL"),
            params![metric, date],
            |r| r.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten()
    };
    let first = |names: &[&str], how: &str| names.iter().find_map(|n| one(n, how));
    // Food totals with what marked supplements add (0.18).
    let food = |metric: &str| -> Option<f64> {
        let add = crate::meds_svc::food_add(conn, metric).get(date).copied();
        match (one(metric, "max"), add) {
            (Some(a), b) => Some(a + b.unwrap_or(0.0)),
            (None, b) => b,
        }
    };
    let core = one("sleep_analysis_core_seconds", "max");
    let deep = one("sleep_analysis_deep_seconds", "max");
    let rem = one("sleep_analysis_rem_seconds", "max");
    let awake = one("sleep_analysis_awake_seconds", "max");
    let total = core.unwrap_or(0.0) + deep.unwrap_or(0.0) + rem.unwrap_or(0.0);
    let hyp = hypnograms(conn, date).into_iter().find(|h| h.date == date);
    // Wrist temperature as the shift from the 30 days before.
    let temp = series(conn, "wrist_temp", "avg");
    let temp_shift = temp.get(date).and_then(|v| median(&crate::engine::window(&temp, date, -30, 0)).map(|m| ((v - m) * 100.0).round() / 100.0));
    let spo2 = spo2_series(conn).0.get(date).copied();
    let fat = one("body_fat_pct", "avg").map(|v| if v <= 1.0 { v * 100.0 } else { v });

    let start = crate::engine::day_num(date).map(|d| d * 86400 - tz).unwrap_or(0);
    let mut st = conn
        .prepare(
            "SELECT a.id, a.sport, a.name, a.start_utc, COALESCE(a.moving_s, a.elapsed_s), a.distance_m, a.avg_hr, m.load
             FROM va a LEFT JOIN activity_metrics m ON m.activity_id = a.id
             WHERE a.start_utc >= ?1 AND a.start_utc < ?2 ORDER BY a.start_utc",
        )
        .map_err(err)?;
    let acts: Vec<Value> = st
        .query_map(params![start, start + 86400], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?,
                      "startUtc": r.get::<_, i64>(3)?, "durationS": r.get::<_, Option<f64>>(4)?, "distanceM": r.get::<_, Option<f64>>(5)?,
                      "avgHr": r.get::<_, Option<f64>>(6)?, "load": r.get::<_, Option<f64>>(7)?.map(|x| x.round())}))
        })
        .map_err(err)?
        .flatten()
        .collect();
    let mut hs = conn.prepare("SELECT key, val FROM habit_log WHERE date = ?1").map_err(err)?;
    let habits: Vec<Value> = hs
        .query_map(params![date], |r| Ok(json!([r.get::<_, String>(0)?, r.get::<_, i64>(1)?])))
        .map_err(err)?
        .flatten()
        .collect();
    let caf_last: Option<String> = {
        let mut st = conn.prepare("SELECT ts FROM hm WHERE metric = 'food_entry_caffeine' AND date = ?1").map_err(err)?;
        let ts: Vec<i64> = st.query_map(params![date], |r| r.get::<_, String>(0)).map_err(err)?.flatten().filter_map(|t| iso(&t)).collect();
        ts.iter().max().map(|t| hhmm(clock_min(*t, tz)))
    };
    let hrv_day = hrv_series(conn).0.get(date).map(|v| v.round());
    let rhr_day = rhr_series(conn).0.get(date).map(|v| v.round());
    let stress_day = {
        let p = engine_svc::profile(conn);
        let all = crate::stress_svc::days(conn, &p, &add_days(date, -30), date);
        all.get(date).map(|s| crate::stress_svc::day_json(s, p.tz_offset_s, crate::stress_svc::norm(&all, date)))
    };
    Ok(json!({
        "date": date,
        "tz": tz,
        "sleep": if total > 600.0 { json!({"totalS": total.round(), "deepS": deep, "coreS": core, "remS": rem, "awakeS": awake}) } else { Value::Null },
        "hypnogram": hyp.map(|h| json!({"date": h.date, "start": h.start, "end": h.end, "segs": h.segs.iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>()})),
        "hrv": hrv_day,
        "rhr": rhr_day,
        "stress": stress_day,
        "meds": crate::meds_svc::taken_on(conn, date),
        "hrRecovery": one("hr_recovery_1min", "avg").map(|v| v.round()),
        "walkHr": one("walking_hr_avg_merged_value", "avg").map(|v| v.round()),
        "resp": one("respiratory_rate_merged_avg", "avg").map(r1),
        "breath": one("sleeping_breathing_disturbances", "avg").map(r1),
        "spo2": spo2.map(r1),
        "tempShift": temp_shift,
        "steps": one("steps_total", "max").map(|v| v.round()),
        "activeKcal": one("active_energy_total", "max").map(|v| v.round()),
        "kcal": food("dietary_energy_total").map(|v| v.round()),
        "protein": food("dietary_protein_total").map(|v| v.round()),
        "carbs": food("dietary_carbohydrates_total").map(|v| v.round()),
        "fatG": food("dietary_fat_total_total").map(|v| v.round()),
        "waterMl": food("dietary_water_total").map(|v| v.round()),
        "alcohol": one("alcoholic_beverages_total", "max"),
        "caffeine": food("dietary_caffeine_total").map(|v| v.round()),
        "caffeineLast": caf_last,
        "vo2": one("vo2_max", "avg").map(r1),
        "weight": one("body_mass", "avg").map(r1),
        "fatPct": fat.map(r1),
        "activities": acts,
        "habits": habits,
    }))
}
