//! Muscle load (a separate track from the heart load) and recovery time per workout.
//!
//! Muscle load estimates what the workout did to muscles, tendons and joints: distance and pace
//! for running and walking, metres up and down (downhill brakes the quads and loads the knees),
//! sets by exercise for strength. It is never added to the heart load — it has its own norm and
//! its own acute:chronic ratio. Values are kept per muscle group for the future muscle map.

use crate::engine::training::LoadDay;
use crate::engine::{add_days, day_of, Profile};
use crate::engine_svc::Cached;
use crate::store;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

const MV: &str = "m3";

pub const GROUPS: [(&str, &str); 12] = [
    ("calves", "Икры"),
    ("quads", "Квадрицепсы"),
    ("hams", "Задняя поверхность бедра"),
    ("glutes", "Ягодицы"),
    ("hips", "Таз и бёдра (стабилизаторы)"),
    ("core", "Кор"),
    ("back", "Спина"),
    ("chest", "Грудь"),
    ("shoulders", "Плечи"),
    ("arms", "Руки"),
    ("knees", "Колени"),
    ("ankles", "Ахиллы и голеностоп"),
];

type G = [f64; 12];
const FLAT: G = [0.28, 0.22, 0.14, 0.14, 0.06, 0.04, 0., 0., 0., 0., 0.06, 0.06];
const UP: G = [0.30, 0.15, 0.10, 0.30, 0.05, 0., 0., 0., 0., 0., 0., 0.10];
const DOWN: G = [0.10, 0.45, 0., 0., 0., 0., 0., 0., 0., 0., 0.35, 0.10];
const GYM: G = [0.03, 0.15, 0.10, 0.15, 0.04, 0.13, 0.15, 0.08, 0.08, 0.09, 0., 0.];
const MOBILITY: G = [0., 0., 0.1, 0.1, 0.2, 0.3, 0.2, 0., 0.1, 0., 0., 0.];
const RIDE: G = [0.1, 0.4, 0.1, 0.25, 0., 0., 0., 0., 0., 0., 0.15, 0.];
/// 1.4.1: elliptical (no impact, arms on the handles) and stair stepper (climbing).
const ELLIPTICAL: G = [0.12, 0.3, 0.13, 0.25, 0.05, 0.05, 0., 0., 0.04, 0.06, 0., 0.];
const STEPPER: G = [0.22, 0.3, 0.08, 0.3, 0.04, 0.02, 0., 0., 0., 0., 0.04, 0.];
const SWIM: G = [0., 0., 0., 0., 0., 0.2, 0.3, 0.05, 0.3, 0.15, 0., 0.];

/// Muscle groups for a FIT exercise category (or the exercise name); None = not recognised
/// (1.4: such sets count for the total muscle load but are not shown on the map).
fn exercise_groups(cat: Option<i64>, name: &str) -> Option<G> {
    let n = name.to_lowercase();
    let has = |k: &[&str]| k.iter().any(|x| n.contains(x));
    let c = cat.unwrap_or(-1);
    let mut g = [0.0; 12];
    let mut put = |i: usize, w: f64| g[i] += w;
    match c {
        28 => { put(1, 0.45); put(3, 0.35); put(5, 0.1); put(10, 0.1) }              // squat
        8 => { put(2, 0.35); put(3, 0.3); put(6, 0.3); put(5, 0.05) }               // deadlift
        17 => { put(1, 0.4); put(3, 0.35); put(4, 0.1); put(10, 0.15) }             // lunge
        1 => { put(0, 0.8); put(11, 0.2) }                                          // calf raise
        15 => put(2, 1.0),                                                          // leg curl
        10 | 11 => { put(3, 0.6); put(4, 0.4) }                                     // hip raise / stability
        0 | 9 | 22 => { put(7, 0.6); put(8, 0.2); put(9, 0.2) }                     // bench / flye / push-up
        21 | 23 => { put(6, 0.65); put(9, 0.35) }                                   // pull-up / row
        24 | 14 | 25 | 26 => { put(8, 0.8); put(9, 0.2) }                           // shoulders
        7 | 30 => put(9, 1.0),                                                      // curl / triceps
        5 | 6 | 19 | 27 | 16 | 4 => put(5, 1.0),                                    // core
        20 => { put(0, 0.35); put(1, 0.3); put(3, 0.2); put(10, 0.1); put(11, 0.05) } // plyo
        _ => {
            if has(&["присед", "squat", "жим ногами", "leg press"]) { put(1, 0.5); put(3, 0.4); put(10, 0.1) }
            else if has(&["станов", "deadlift", "румын"]) { put(2, 0.4); put(3, 0.3); put(6, 0.3) }
            else if has(&["выпад", "lunge", "болгар"]) { put(1, 0.45); put(3, 0.4); put(10, 0.15) }
            else if has(&["икр", "calf"]) { put(0, 1.0) }
            else if has(&["жим", "отжим", "bench", "push"]) { put(7, 0.6); put(8, 0.2); put(9, 0.2) }
            else if has(&["тяг", "подтяг", "row", "pull"]) { put(6, 0.65); put(9, 0.35) }
            else if has(&["плеч", "shoulder", "мах"]) { put(8, 1.0) }
            else if has(&["бицепс", "трицепс", "curl"]) { put(9, 1.0) }
            else if has(&["планк", "пресс", "plank", "crunch", "core"]) { put(5, 1.0) }
            else { return None }
        }
    }
    Some(g)
}

fn add(acc: &mut G, w: &G, k: f64) {
    for i in 0..12 {
        acc[i] += w[i] * k;
    }
}

/// Metres up and down from the altitude stream: median-smoothed (9 points) against spikes, then
/// counted with a 5 m hysteresis.
fn up_down(raw: &[f64]) -> (f64, f64) {
    let (mut up, mut down) = (0.0, 0.0);
    if raw.is_empty() {
        return (0.0, 0.0);
    }
    let alt: Vec<f64> = (0..raw.len())
        .map(|i| {
            let a = i.saturating_sub(4);
            let b = (i + 5).min(raw.len());
            let mut w: Vec<f64> = raw[a..b].to_vec();
            w.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            w[w.len() / 2]
        })
        .collect();
    let mut base = alt[0];
    for &a in &alt {
        let d = a - base;
        if d >= 5.0 {
            up += d;
            base = a;
        } else if d <= -5.0 {
            down -= d;
            base = a;
        }
    }
    (up, down)
}

pub struct MLoad {
    pub id: String,
    pub sport: String,
    pub start: i64,
    pub end: i64,
    pub total: f64,
    pub groups: G,
    pub up: f64,
    pub down: f64,
    /// The altimeter failed; relief replaced by your usual for this sport.
    pub bad: bool,
    /// 1.4: load with no known muscles (strength / ОФП without entered exercises). Counted in
    /// `total` (ACWR, injury risk, recovery) but not on the muscle map.
    pub free: f64,
}

/// Muscle load of every workout (cached per workout in `muscle_cache`).
/// Most metres per km that are believable here; above it the altimeter has failed.
fn cap_per_km(sport: &str) -> f64 {
    match sport {
        "run" => 40.0,
        "hike" => 70.0,
        _ => 30.0,
    }
}

pub fn loads(conn: &Connection, acts: &[Cached], p: &Profile) -> Vec<MLoad> {
    let v_ref = 1000.0 / p.threshold_pace_s.unwrap_or(300.0).max(150.0);
    let cache: HashMap<String, (String, String)> = conn
        .prepare("SELECT activity_id, version, data FROM muscle_cache")
        .map(|mut st| {
            let v: HashMap<String, (String, String)> = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?))))
                .map(|r| r.flatten().collect())
                .unwrap_or_default();
            v
        })
        .unwrap_or_default();
    let meta: HashMap<String, (Option<f64>, Option<f64>, Option<f64>, i64)> = conn
        .prepare("SELECT id, elev_gain_m, rpe, elapsed_s, has_streams FROM va")
        .map(|mut st| {
            let v: HashMap<String, (Option<f64>, Option<f64>, Option<f64>, i64)> = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))))
                .map(|r| r.flatten().collect())
                .unwrap_or_default();
            v
        })
        .unwrap_or_default();

    // Stage 1: metres up/down as recorded (stream parse is the slow part, cached by version).
    let mut raw: Vec<(f64, f64, Option<Value>)> = Vec::with_capacity(acts.len());
    for a in acts {
        let (elev, _, _, has_streams) = meta.get(&a.id).cloned().unwrap_or((None, None, None, 0));
        let ver = format!("{MV}:{has_streams}:{:.0}", elev.unwrap_or(-1.0));
        let old = cache.get(&a.id).and_then(|(v, d)| serde_json::from_str::<Value>(d).ok().map(|j| (v.clone(), j)));
        if let Some((v, j)) = &old {
            if *v == ver && j["ru"].is_number() {
                raw.push((j["ru"].as_f64().unwrap_or(0.0), j["rd"].as_f64().unwrap_or(0.0), Some(j.clone())));
                continue;
            }
        }
        let (mut up, mut down) = (0.0, 0.0);
        if matches!(a.sport.as_str(), "run" | "walk" | "hike") {
            if has_streams == 1 {
                if let Some(js) = store::streams_json(conn, &a.id) {
                    if let Ok(v) = serde_json::from_str::<Value>(&js) {
                        let alt: Vec<f64> = v["alt"].as_array().map(|x| x.iter().filter_map(|y| y.as_f64()).collect()).unwrap_or_default();
                        (up, down) = up_down(&alt);
                    }
                }
            }
            if let Some(e) = elev {
                // Strava's own gain is smoothed; a stream far above it is noise.
                if up == 0.0 || up > e * 1.5 + 10.0 {
                    let k = if up > 0.0 { down / up } else { 1.0 };
                    up = e;
                    down = e * k;
                }
            }
        }
        let j = json!({"ru": up.round(), "rd": down.round(), "rec": old.as_ref().map(|o| o.1["rec"].clone()).unwrap_or(Value::Null)});
        let _ = conn.execute(
            "INSERT INTO muscle_cache (activity_id, version, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(activity_id) DO UPDATE SET version = excluded.version, data = excluded.data",
            params![a.id, ver, j.to_string()],
        );
        raw.push((up.round(), down.round(), Some(j)));
    }

    // Your usual relief per sport (median metres per km over believable workouts).
    let mut rates: HashMap<&str, Vec<f64>> = HashMap::new();
    for (a, r) in acts.iter().zip(&raw) {
        let km = a.distance_m.unwrap_or(0.0) / 1000.0;
        if km >= 1.0 && r.0 / km <= cap_per_km(&a.sport) {
            rates.entry(a.sport.as_str()).or_default().push(r.0 / km);
        }
    }
    let typical = |sport: &str| -> f64 { rates.get(sport).and_then(|v| crate::engine::median(v)).unwrap_or(8.0) };

    // Stage 2: muscle load from distance, pace, believable relief and sets (cheap, every time).
    let mut out = Vec::new();
    for (a, r) in acts.iter().zip(&raw) {
        let (_, rpe, elapsed, _) = meta.get(&a.id).cloned().unwrap_or((None, None, None, 0));
        let end = a.start_utc + elapsed.unwrap_or(a.duration_s) as i64;
        let mut g = [0.0; 12];
        let km = a.distance_m.unwrap_or(0.0) / 1000.0;
        let (mut up, mut down) = (r.0, r.1);
        let mut bad = false;
        let mut free = 0.0;
        if km > 0.3 && up / km > cap_per_km(&a.sport) {
            bad = true;
            up = (typical(&a.sport) * km).round();
            down = up;
        }
        let session = a.duration_s / 60.0 * rpe.unwrap_or(5.0) * 0.45;
        match a.sport.as_str() {
            "run" => {
                let speed = if a.duration_s > 0.0 { a.distance_m.unwrap_or(0.0) / a.duration_s } else { 0.0 };
                let sf = (speed / v_ref).clamp(0.4, 1.6).powf(1.5);
                add(&mut g, &FLAT, km * 8.0 * sf);
                add(&mut g, &UP, up / 100.0 * 12.0);
                add(&mut g, &DOWN, down / 100.0 * 15.0);
            }
            "walk" | "hike" => {
                let k = if a.sport == "hike" { 1.2 } else { 1.0 };
                add(&mut g, &FLAT, km * 1.5 * k);
                add(&mut g, &UP, up / 100.0 * 6.0 * k);
                add(&mut g, &DOWN, down / 100.0 * 7.0 * k);
            }
            "strength" | "workout" => {
                let total = session * if a.sport == "strength" { 0.8 } else { 0.6 };
                let sets = sets_of(conn, &a.id);
                if sets.is_empty() {
                    // 1.4 (owner): no exercises → no map fill; ОФП without exercises as before (× 0.4).
                    free = if a.sport == "strength" { total } else { session * 0.4 };
                } else {
                    let per = total / sets.len() as f64;
                    for w in &sets {
                        match w {
                            Some(w) => add(&mut g, w, per),
                            None => free += per,
                        }
                    }
                }
            }
            "yoga" => add(&mut g, &MOBILITY, session * 0.4),
            "ride" | "ride_virtual" => add(&mut g, &RIDE, a.load * 0.5),
            "swim" => add(&mut g, &SWIM, a.load * 0.5),
            "elliptical" => add(&mut g, &ELLIPTICAL, a.load * 0.5),
            "stepper" => add(&mut g, &STEPPER, a.load * 0.6),
            _ => add(&mut g, &GYM, a.load * 0.3),
        }
        // 1.4: ОФП or a complex entered on a run / walk / ride — about 2 units per set on top.
        if !matches!(a.sport.as_str(), "strength" | "workout" | "yoga") {
            for w in sets_of(conn, &a.id) {
                match w {
                    Some(w) => add(&mut g, &w, 2.0),
                    None => free += 2.0,
                }
            }
        }
        let total: f64 = g.iter().sum::<f64>() + free;
        // Keep the page's copy in step (written only when it changed).
        if let Some(j) = &r.2 {
            if (j["t"].as_f64().unwrap_or(-1.0) - total.round()).abs() >= 1.0 || j["bad"].as_bool() != Some(bad) || j["u"].as_f64() != Some(up) || (j["f"].as_f64().unwrap_or(0.0) - free.round()).abs() >= 1.0 {
                let mut j = j.clone();
                j["t"] = json!(total.round());
                j["g"] = json!(g.iter().map(|x| (x * 10.0).round() / 10.0).collect::<Vec<_>>());
                j["u"] = json!(up);
                j["d"] = json!(down);
                j["bad"] = json!(bad);
                j["f"] = json!(free.round());
                let _ = conn.execute("UPDATE muscle_cache SET data = ?2 WHERE activity_id = ?1", params![a.id, j.to_string()]);
            }
        }
        out.push(MLoad { id: a.id.clone(), sport: a.sport.clone(), start: a.start_utc, end, total, groups: g, up, down, bad, free });
    }
    out
}

/// Muscles of each done set of a workout (None = exercise not recognised). Rest and skipped
/// rows are left out.
fn sets_of(conn: &Connection, id: &str) -> Vec<Option<G>> {
    let custom = crate::exercise_svc::custom_snapshot();
    let rows: Vec<(Option<i64>, String, Option<String>)> = conn
        .prepare("SELECT category, COALESCE(exercise, ''), ex_id FROM strength_sets WHERE activity_id = ?1 AND COALESCE(kind, 'active') = 'active'")
        .map(|mut st| {
            let v: Vec<(Option<i64>, String, Option<String>)> =
                st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
            v
        })
        .unwrap_or_default();
    rows.iter()
        .map(|(c, n, x)| {
            // 0.22: an exercise from the library (1.4: or your own) brings its own muscles.
            x.as_deref().and_then(|x| crate::exercise_svc::groups_in(x, &custom)).or_else(|| exercise_groups(*c, n))
        })
        .collect()
}

/// Strength / ОФП workouts of the last `days` days whose load is mostly not on the map
/// (no exercises entered or names not recognised) — the map asks to enter them.
pub fn missing(ml: &[MLoad], names: &HashMap<String, Option<String>>, days: i64) -> Vec<Value> {
    let from = store::now() - days * 86400;
    ml.iter()
        .filter(|m| m.start >= from && m.free >= 1.0 && m.free >= 0.4 * m.total)
        .map(|m| json!({"id": m.id, "sport": m.sport, "start": m.start, "name": names.get(&m.id).cloned().flatten(), "free": m.free.round(), "total": m.total.round()}))
        .collect()
}

/// Daily muscle totals by local date.
pub fn daily(ml: &[MLoad], tz: i64) -> BTreeMap<String, f64> {
    let mut d: BTreeMap<String, f64> = BTreeMap::new();
    for m in ml {
        *d.entry(day_of(m.start, tz)).or_default() += m.total;
    }
    d
}

/// Usual muscle load per day: average over the 28 days before `date`.
pub fn usual_day(daily: &BTreeMap<String, f64>, date: &str) -> f64 {
    let s: f64 = daily.range(add_days(date, -28)..date.to_string()).map(|(_, v)| v).sum();
    s / 28.0
}

/// Acute:chronic for muscles — last 7 days vs the average week of the 28 before them (no data → None).
pub fn acwr(daily: &BTreeMap<String, f64>, today: &str) -> (Option<f64>, f64, f64) {
    let week: f64 = daily.range(add_days(today, -6)..add_days(today, 1)).map(|(_, v)| v).sum();
    let chronic: f64 = daily.range(add_days(today, -34)..add_days(today, -6)).map(|(_, v)| v).sum::<f64>() / 4.0;
    let first = daily.keys().next().cloned().unwrap_or_else(|| today.to_string());
    let enough = first <= add_days(today, -21) && chronic > 0.0;
    (if enough { Some(week / chronic) } else { None }, week, chronic)
}

/// Recovery time of one workout, hours: the longer of heart and muscles (+4 h when both are big).
pub fn recovery_hours(a: &Cached, m: Option<&MLoad>, ctl: f64, m_usual: f64) -> (f64, f64, f64) {
    let r = (a.load / ctl.max(1.0)).min(3.0);
    let z45 = (a.zone_s[4] + a.zone_s[5]) / 60.0;
    let heart = (3.0 + 14.0 * r + 0.6 * z45).clamp(1.0, 72.0);
    // Against a usual muscle day of at least 10, and never more than 3× it.
    let muscle = m.map(|m| (3.0 + 14.0 * (m.total / m_usual.max(10.0)).min(3.0)).clamp(1.0, 72.0)).unwrap_or(0.0);
    let both = if heart > 18.0 && muscle > 18.0 { 4.0 } else { 0.0 };
    let mut h = (heart.max(muscle) + both).clamp(1.0, 96.0);
    if matches!(a.sport.as_str(), "walk") {
        h = h.min(12.0);
    }
    (h.round(), heart.round(), muscle.round())
}

/// Recovery of recent workouts and what is left now. `ready` = today's morning score: a good
/// morning shortens what is left, a poor one stretches it (±30%).
pub fn recovery(acts: &[Cached], ml: &[MLoad], ls: &[LoadDay], mdaily: &BTreeMap<String, f64>, tz: i64, ready: Option<f64>) -> Value {
    let now = store::now();
    let by_id: HashMap<&str, &MLoad> = ml.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut list: Vec<(f64, f64, Value)> = Vec::new();
    for a in acts.iter().filter(|a| a.start_utc >= now - 6 * 86400) {
        let d = day_of(a.start_utc, tz);
        let ctl = ls.iter().filter(|l| l.date < d).last().map(|l| l.ctl).unwrap_or(1.0);
        let m = by_id.get(a.id.as_str()).copied();
        let (h, hh, mh) = recovery_hours(a, m, ctl, usual_day(mdaily, &d));
        let end = m.map(|m| m.end).unwrap_or(a.start_utc + a.duration_s as i64);
        let left = (h - (now - end) as f64 / 3600.0).max(0.0);
        list.push((left, h, json!({"id": a.id, "sport": a.sport, "end": end, "hours": h, "heartH": hh, "muscleH": mh, "leftH": left.round(),
                                   "muscle": m.map(|m| m.total.round()), "up": m.map(|m| m.up), "down": m.map(|m| m.down), "bad": m.map(|m| m.bad)})));
    }
    list.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    // The longest one, plus 30% of others that ended within a day of it (back-to-back sessions).
    let main_end = list.first().and_then(|x| x.2["end"].as_i64()).unwrap_or(0);
    let raw = list.first().map(|x| x.0).unwrap_or(0.0)
        + 0.3 * list.iter().skip(1).filter(|x| (x.2["end"].as_i64().unwrap_or(0) - main_end).abs() <= 86400).map(|x| x.0).sum::<f64>();
    let f = ready.map(|s| (1.0 - (s - 50.0) / 100.0).clamp(0.7, 1.3)).unwrap_or(1.0);
    json!({"leftH": (raw * f).round(), "factor": (f * 100.0).round() / 100.0, "workouts": list.into_iter().map(|x| x.2).collect::<Vec<_>>()})
}

/// One workout's muscle load and recovery for its page (from the cache written by the summary).
pub fn for_activity(conn: &Connection, id: &str) -> Value {
    let data: Option<String> = conn.query_row("SELECT data FROM muscle_cache WHERE activity_id = ?1", params![id], |r| r.get(0)).ok();
    let Some(v) = data.and_then(|d| serde_json::from_str::<Value>(&d).ok()) else { return Value::Null };
    let g: Vec<f64> = v["g"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
    let total = v["t"].as_f64().unwrap_or(0.0);
    // Shares of what is on the map (1.4: load without exercises is not part of it).
    let mapped = total - v["f"].as_f64().unwrap_or(0.0);
    let mut top: Vec<(usize, f64)> = g.iter().copied().enumerate().filter(|x| x.1 > 0.0).collect();
    top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    json!({
        "total": total.round(), "free": v["f"], "up": v["u"], "down": v["d"], "rec": v["rec"], "bad": v["bad"],
        "groups": top.iter().map(|(i, x)| json!({"key": GROUPS[*i].0, "name": GROUPS[*i].1, "value": x.round(), "pct": if mapped > 0.0 { (x / mapped * 100.0).round() } else { 0.0 }})).collect::<Vec<_>>(),
    })
}

/// Saves recovery hours next to the muscle data so the workout page can show them.
pub fn store_recovery(conn: &Connection, rec: &Value) {
    if let Some(list) = rec["workouts"].as_array() {
        for w in list {
            if let Some(id) = w["id"].as_str() {
                let r = json!({"hours": w["hours"], "heartH": w["heartH"], "muscleH": w["muscleH"]}).to_string();
                let _ = conn.execute(
                    "UPDATE muscle_cache SET data = json_set(data, '$.rec', json(?2)) WHERE activity_id = ?1",
                    params![id, r],
                );
            }
        }
    }
}

/// State of each muscle group now, for the muscle map: what is still "in" the muscle
/// (load × e^(−hours/36)) against your usual steady level for that group (28-day daily average
/// × 2.05, the level a daily load of that size settles at). Ratio: <0.6 свежие, ≤1.2 в норме,
/// ≤1.6 нагружены, above — перегружены. Contributors: the workouts that put most of it there.
pub fn map_state(ml: &[MLoad], names: &HashMap<String, Option<String>>) -> Vec<Value> {
    let now = store::now();
    let tau = 36.0 * 3600.0;
    let from28 = now - 28 * 86400;
    (0..12)
        .map(|g| {
            let usual = ml.iter().filter(|m| m.start >= from28 && m.start < now).map(|m| m.groups[g]).sum::<f64>() / 28.0;
            let mut contrib: Vec<(f64, &MLoad)> = ml
                .iter()
                .filter(|m| m.start >= now - 6 * 86400 && m.groups[g] > 0.0)
                .map(|m| (m.groups[g] * (-((now - m.end).max(0) as f64) / tau).exp(), m))
                .collect();
            let acute: f64 = contrib.iter().map(|c| c.0).sum();
            let steady = usual * 2.05;
            let ratio = if steady > 0.5 { Some(acute / steady) } else { None };
            let state = match ratio {
                None if acute < 1.0 => "fresh",
                None => "loaded",
                Some(r) if r < 0.6 => "fresh",
                Some(r) if r <= 1.2 => "normal",
                Some(r) if r <= 1.6 => "loaded",
                Some(_) => "over",
            };
            contrib.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            json!({
                "key": GROUPS[g].0, "name": GROUPS[g].1, "state": state,
                "ratio": ratio.map(|r| (r * 100.0).round() / 100.0), "now": (acute * 10.0).round() / 10.0, "usual": (steady * 10.0).round() / 10.0,
                "contrib": contrib.iter().take(4).map(|(v, m)| json!({"id": m.id, "sport": m.sport, "start": m.start, "name": names.get(&m.id).cloned().flatten(),
                    "share": if acute > 0.0 { (v / acute * 100.0).round() } else { 0.0 }})).collect::<Vec<_>>(),
            })
        })
        .collect()
}
