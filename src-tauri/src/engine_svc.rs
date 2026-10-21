//! Glue between the database and the engine: loads series, caches per-workout
//! metrics and assembles the summary the interface shows.

use crate::engine::daily::{self, HealthSeries};
use crate::engine::training::{self, ActInput};
use crate::engine::{add_days, day_of, latest, median, window, Profile, Series};
use crate::{db, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

/// Bump when a formula changes so cached workout metrics are recomputed.
const ENGINE_V: &str = "e1";

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------- profile ----------

pub fn profile(conn: &Connection) -> Profile {
    let mut p = Profile::default();
    p.tz_offset_s = chrono::Local::now().offset().local_minus_utc() as i64;
    let Some(raw) = db::get_setting(conn, "profile") else { return p };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else { return p };
    // A profile saved before 0.18 was always the owner's own, so it counts as filled in.
    p.filled = v["filled"].as_bool().unwrap_or(true);
    p.name = v["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    if let Some(x) = v["hrMax"].as_f64() {
        p.hr_max = x;
        if v["zones"].as_array().map(|z| z.len() != 6).unwrap_or(true) {
            p.zones = crate::engine::zones_from_max(x);
        }
    }
    p.hr_rest = v["hrRest"].as_f64();
    if let Some(z) = v["zones"].as_array() {
        if z.len() == 6 {
            for (i, x) in z.iter().enumerate() {
                if let Some(f) = x.as_f64() {
                    p.zones[i] = f;
                }
            }
        }
    }
    if let Some(x) = v["heightCm"].as_f64() {
        p.height_cm = x;
    }
    if let Some(x) = v["birthYear"].as_i64() {
        p.birth_year = x;
    }
    if let Some(x) = v["male"].as_bool() {
        p.male = x;
    }
    if let Some(x) = v["weekCap"].as_f64() {
        p.week_cap = x.clamp(1.0, 1.6);
    }
    if let Some(z) = v["paceZones"].as_array() {
        if z.len() == 6 && z.iter().all(|x| x.as_f64().is_some()) {
            for (i, x) in z.iter().enumerate() {
                p.pace_zones[i] = x.as_f64().unwrap_or(p.pace_zones[i]);
            }
        }
    }
    // Old saves carry "proteinGPerKg" (the effective value); only "proteinManual" is an override.
    p.protein_manual = v["proteinManual"].as_f64().filter(|x| (0.8..=3.5).contains(x));
    if let Some(g) = v["goal"].as_str().filter(|g| matches!(*g, "maintain" | "lose" | "gain")) {
        p.goal = g.to_string();
    }
    if let Some(x) = v["goalRate"].as_f64() {
        p.goal_rate = x.clamp(0.1, 1.0);
    }
    p.target_weight = v["targetWeight"].as_f64().filter(|x| (40.0..=250.0).contains(x));
    p.protein_g_per_kg = p.protein_manual.unwrap_or_else(|| crate::engine::protein_for_goal(&p.goal));
    p.zones_date = v["zonesDate"].as_str().map(|s| s.to_string()).or(p.zones_date);
    if let Some(x) = v["thresholdPaceS"].as_f64() {
        p.threshold_pace_s = Some(x);
        if v["paceZones"].as_array().map(|z| z.len() != 6).unwrap_or(true) {
            p.pace_zones = crate::engine::pace_from_threshold(x);
        }
    }
    p
}

pub fn profile_json(p: &Profile) -> Value {
    json!({
        "name": p.name, "filled": p.filled, "hrMax": p.hr_max, "hrRest": p.hr_rest, "zones": p.zones, "heightCm": p.height_cm,
        "birthYear": p.birth_year, "male": p.male, "proteinGPerKg": p.protein_g_per_kg, "proteinManual": p.protein_manual,
        "goal": p.goal, "goalRate": p.goal_rate, "targetWeight": p.target_weight, "weekCap": p.week_cap,
        "paceZones": p.pace_zones, "zonesDate": p.zones_date, "thresholdPaceS": p.threshold_pace_s,
    })
}

pub fn save_profile(conn: &Connection, v: &Value) -> Result<(), String> {
    // Validate by round-tripping through the Profile parser.
    db::set_setting(conn, "profile", &v.to_string())?;
    let p = profile(conn);
    if !(p.hr_max > 100.0 && p.hr_max < 230.0) {
        return Err("Максимальный пульс должен быть от 100 до 230".into());
    }
    if p.zones.windows(2).any(|w| w[1] <= w[0]) {
        return Err("Границы зон должны идти по возрастанию".into());
    }
    if p.pace_zones.windows(2).any(|w| w[1] >= w[0]) {
        return Err("Границы темпа должны идти от медленного к быстрому".into());
    }
    db::set_setting(conn, "profile", &profile_json(&p).to_string())
}

// ---------- series ----------

/// One value per day for a metric. `how`: "avg", "median", "sum", "max".
pub fn series(conn: &Connection, metric: &str, how: &str) -> Series {
    let agg = match how {
        "sum" => "sum(num)",
        "max" => "max(num)",
        _ => "avg(num)",
    };
    let mut out = Series::new();
    if how == "median" {
        let Ok(mut st) = conn.prepare("SELECT date, num FROM hm WHERE metric = ?1 AND num IS NOT NULL ORDER BY date")
        else {
            return out;
        };
        let mut by: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        if let Ok(rows) = st.query_map(params![metric], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))) {
            for (d, v) in rows.flatten() {
                by.entry(d).or_default().push(v);
            }
        }
        for (d, v) in by {
            if let Some(m) = median(&v) {
                out.insert(d, m);
            }
        }
        return out;
    }
    let sql = format!("SELECT date, {agg} FROM hm WHERE metric = ?1 AND num IS NOT NULL GROUP BY date");
    let Ok(mut st) = conn.prepare(&sql) else { return out };
    if let Ok(rows) = st.query_map(params![metric], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))) {
        for (d, v) in rows.flatten() {
            out.insert(d, v);
        }
    }
    // 0.18: what marked supplements add (isotonic carbs and water, caffeine from a pre-workout).
    for (d, v) in crate::meds_svc::food_add(conn, metric) {
        *out.entry(d).or_insert(0.0) += v;
    }
    out
}

#[allow(dead_code)]
fn first_nonempty(conn: &Connection, names: &[&str], how: &str) -> Series {
    for n in names {
        let s = series(conn, n, how);
        if !s.is_empty() {
            return s;
        }
    }
    Series::new()
}

pub fn today(p: &Profile) -> String {
    day_of(store::now(), p.tz_offset_s)
}

pub fn resting_hr(conn: &Connection, p: &Profile) -> f64 {
    if let Some(r) = p.hr_rest {
        return r;
    }
    let s = series(conn, "resting_heart_rate_merged_avg", "avg");
    let t = today(p);
    median(&window(&s, &t, -30, 1)).map(|x| x.round()).unwrap_or(52.0)
}

// ---------- per-workout metrics cache ----------

fn stream_pairs(js: &str) -> (Vec<(i64, f64)>, Vec<(i64, f64)>) {
    let Ok(v) = serde_json::from_str::<Value>(js) else { return (vec![], vec![]) };
    let t: Vec<i64> = v["t"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
    let pick = |k: &str| -> Vec<(i64, f64)> {
        v[k].as_array()
            .map(|a| a.iter().zip(t.iter()).filter_map(|(x, tt)| x.as_f64().map(|f| (*tt, f))).collect())
            .unwrap_or_default()
    };
    (pick("hr"), pick("dist"))
}

pub struct Cached {
    pub id: String,
    pub sport: String,
    pub start_utc: i64,
    pub duration_s: f64,
    pub distance_m: Option<f64>,
    pub calories: Option<f64>,
    pub load: f64,
    pub method: String,
    pub zone_s: [f64; 6],
    pub best: Vec<(f64, f64)>,
}

/// Computes metrics for workouts that are new or were computed with other settings.
pub fn refresh_metrics(conn: &mut Connection, p: &Profile, rest: f64) -> Result<Vec<Cached>, String> {
    let version = format!("{ENGINE_V}:{}:{}:{:?}:{}", p.hr_max, rest, p.zones, p.male);
    let rows: Vec<(String, String, i64, Option<f64>, Option<f64>, Option<f64>, Option<f64>, i64, Option<String>)> = {
        let mut st = conn
            .prepare(
                "SELECT a.id, a.sport, a.start_utc, COALESCE(a.moving_s, a.elapsed_s), a.distance_m, a.avg_hr, a.rpe,
                        a.has_streams, m.version
                 FROM va a LEFT JOIN activity_metrics m ON m.activity_id = a.id ORDER BY a.start_utc",
            )
            .map_err(err)?;
        let out = st
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        out
    };
    let tx = conn.transaction().map_err(err)?;
    for (id, sport, start, dur, dist, avg_hr, rpe, has, ver) in &rows {
        // The felt-effort rating is part of the key, so rating a workout recomputes it.
        let row_version = format!("{version}:{}", rpe.map(|r| r.to_string()).unwrap_or_default());
        if ver.as_deref() == Some(row_version.as_str()) {
            continue;
        }
        let (hr, dst) = if *has == 1 { store::streams_json(&tx, id).map(|j| stream_pairs(&j)).unwrap_or_default() } else { (vec![], vec![]) };
        let m = training::activity_metrics(
            p,
            rest,
            &ActInput {
                id: id.clone(),
                sport: sport.clone(),
                start_utc: *start,
                duration_s: dur.unwrap_or(0.0),
                distance_m: *dist,
                avg_hr: *avg_hr,
                hr,
                dist: dst,
                rpe: *rpe,
            },
        );
        tx.execute(
            "INSERT INTO activity_metrics (activity_id, version, load, method, zone_s, best) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(activity_id) DO UPDATE SET version = excluded.version, load = excluded.load,
                method = excluded.method, zone_s = excluded.zone_s, best = excluded.best",
            params![
                id,
                row_version,
                m.load,
                m.method,
                json!(m.zone_s).to_string(),
                json!(m.best.iter().map(|(d, t)| [*d, *t]).collect::<Vec<_>>()).to_string()
            ],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;

    let mut st = conn
        .prepare(
            "SELECT a.id, a.sport, a.start_utc, COALESCE(a.moving_s, a.elapsed_s), a.distance_m, a.calories,
                    m.load, m.method, m.zone_s, m.best
             FROM va a JOIN activity_metrics m ON m.activity_id = a.id ORDER BY a.start_utc",
        )
        .map_err(err)?;
    let out = st
        .query_map([], |r| {
            let zs: String = r.get(8)?;
            let bs: String = r.get(9)?;
            let zv: Vec<f64> = serde_json::from_str(&zs).unwrap_or_default();
            let mut zone_s = [0.0; 6];
            for (i, x) in zv.iter().take(6).enumerate() {
                zone_s[i] = *x;
            }
            let bv: Vec<[f64; 2]> = serde_json::from_str(&bs).unwrap_or_default();
            Ok(Cached {
                id: r.get(0)?,
                sport: r.get(1)?,
                start_utc: r.get(2)?,
                duration_s: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                distance_m: r.get(4)?,
                calories: r.get(5)?,
                load: r.get(6)?,
                method: r.get(7)?,
                zone_s,
                best: bv.into_iter().map(|x| (x[0], x[1])).collect(),
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err);
    out
}

// ---------- summary ----------

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub struct HealthData {
    pub hrv: Series,
    pub rhr: Series,
    pub sleep_s: Series,
    pub resp: Series,
    pub breath: Series,
    pub alcohol: Series,
    pub nights: Vec<daily::Night>,
    pub hrv_nightly: bool,
    pub rhr_nightly: bool,
    pub cuts: std::collections::BTreeMap<String, String>,
}

/// The night-based series readiness is built from.
pub fn health_data(conn: &Connection) -> HealthData {
    let (hrv, hrv_nightly) = crate::health_svc::hrv_series(conn);
    let (rhr, rhr_nightly) = crate::health_svc::rhr_series(conn);
    let core = series(conn, "sleep_analysis_core_seconds", "max");
    let deep = series(conn, "sleep_analysis_deep_seconds", "max");
    let rem = series(conn, "sleep_analysis_rem_seconds", "max");
    let awake = series(conn, "sleep_analysis_awake_seconds", "max");
    let mut sleep_s = Series::new();
    let mut nights = Vec::new();
    for d in core.keys().chain(deep.keys()).collect::<std::collections::BTreeSet<_>>() {
        if let Some(n) = daily::night(d, &core, &deep, &rem, &awake) {
            sleep_s.insert(d.clone(), n.total_s);
            nights.push(n);
        }
    }
    HealthData {
        hrv,
        rhr,
        sleep_s,
        resp: series(conn, "respiratory_rate_merged_avg", "avg"),
        breath: series(conn, "sleeping_breathing_disturbances", "avg"),
        alcohol: series(conn, "alcoholic_beverages_total", "max"),
        nights,
        hrv_nightly,
        rhr_nightly,
        cuts: crate::sources::cuts(conn),
    }
}

/// Main vitals for a day against your corridor (30 days before it). `carry` lets today fall back
/// to yesterday's value while tonight is not in yet.
fn vitals_json(d: &str, hrv: &Series, rhr: &Series, resp: &Series, spo2: &Series, temp: &Series, carry: bool, cuts: &std::collections::BTreeMap<String, String>) -> Value {
    let one = |key: &str, label: &str, unit: &str, s: &Series, floor: f64, digits: i32| -> Value {
        let v = s.get(d).copied().or_else(|| if carry { s.get(&add_days(d, -1)).copied() } else { None });
        let rnd = |x: f64| {
            let k = 10f64.powi(digits);
            (x * k).round() / k
        };
        // Not earlier than the switch to the current data source.
        let cut = cuts.get(match key { "hrv" => "hrv", "rhr" => "rhr", _ => "vitals" }).filter(|c| d >= c.as_str());
        let hist: Vec<f64> = match cut {
            Some(c) => s.range(std::cmp::max(add_days(d, -30), c.clone())..d.to_string()).map(|(_, v)| *v).collect(),
            None => window(s, d, -30, 0),
        };
        let c = daily::corridor(&hist, floor);
        let state = match (v, c) {
            (Some(v), Some((m, lo, hi))) => {
                let w = hi - m;
                match key {
                    "hrv" if v < lo => if v < m - 2.0 * w { "bad" } else { "worse" },
                    "hrv" if v > hi => "better",
                    "rhr" if v > hi => if v - m >= 5.0 { "bad" } else { "worse" },
                    "rhr" if v < lo => "better",
                    "resp" if v > hi => if v > m + 2.0 * w { "bad" } else { "worse" },
                    "spo2" if v < lo => if v < m - 2.0 { "bad" } else { "worse" },
                    "temp" if v > hi => if v - m >= 0.5 { "bad" } else { "worse" },
                    _ => "normal",
                }
            }
            _ => "unknown",
        };
        json!({"key": key, "label": label, "unit": unit, "value": v.map(rnd),
               "base": c.map(|x| rnd(x.0)), "lo": c.map(|x| rnd(x.1)), "hi": c.map(|x| rnd(x.2)), "state": state})
    };
    json!([
        one("hrv", "HRV", "мс", hrv, 6.0, 0),
        one("rhr", "Пульс покоя", "уд/мин", rhr, 2.0, 0),
        one("resp", "Дыхание во сне", "/мин", resp, 0.6, 1),
        one("spo2", "Кислород (SpO₂)", "%", spo2, 1.0, 1),
        one("temp", "Температура запястья", "°C", temp, 0.3, 2),
    ])
}

/// Readiness of any day for the History day card: frozen morning (or computed), its factors,
/// the evening score if it was saved, vitals with corridors and that day's questionnaires.
pub fn readiness_day(conn: &mut Connection, date: &str) -> Result<Value, String> {
    let p = profile(conn);
    let rest = resting_hr(conn, &p);
    let acts = refresh_metrics(conn, &p, rest)?;
    let today = today(&p);
    let tz = p.tz_offset_s;
    let dl = training::daily_load(&acts.iter().map(|a| (day_of(a.start_utc, tz), a.load)).collect::<Vec<_>>());
    let ls = training::load_series(&dl, &today);
    let tsb = ls.iter().find(|l| l.date == date).map(|l| l.tsb);
    let hd = health_data(conn);
    let h = HealthSeries { hrv: &hd.hrv, rhr: &hd.rhr, sleep_s: &hd.sleep_s, resp: &hd.resp, breath: &hd.breath, alcohol: &hd.alcohol, cuts: &hd.cuts };
    let r = daily::readiness(date, &h, tsb);
    let mut morning = json!({
        "score": r.score, "level": r.level, "frozen": false,
        "components": r.components.iter().map(|c| json!({"key": c.key, "label": c.label, "value": c.value, "baseline": c.baseline, "score": c.score.map(|s| s.round()), "weight": c.weight, "note": c.note})).collect::<Vec<_>>(),
        "penalties": r.penalties.iter().map(|(t, v)| json!({"text": t, "points": v})).collect::<Vec<_>>(),
    });
    if let Ok((sc, lvl, data)) = conn.query_row(
        "SELECT score, level, data FROM readiness_log WHERE date = ?1",
        params![date],
        |q| Ok((q.get::<_, Option<f64>>(0)?, q.get::<_, String>(1)?, q.get::<_, String>(2)?)),
    ) {
        morning["score"] = json!(sc);
        morning["level"] = json!(lvl);
        morning["frozen"] = json!(true);
        if let Ok(v) = serde_json::from_str::<Value>(&data) {
            if v["components"].is_array() {
                morning["components"] = v["components"].clone();
            }
            if v["penalties"].is_array() {
                morning["penalties"] = v["penalties"].clone();
            }
        }
    }
    let evening: Value = conn
        .query_row("SELECT data FROM evening_log WHERE date = ?1", params![date], |q| q.get::<_, String>(0))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let psych: Vec<Value> = {
        let mut st = conn.prepare("SELECT kind, score, extra, ts FROM surveys WHERE date = ?1 ORDER BY ts").map_err(err)?;
        let v = st
            .query_map(params![date], |q| {
                let extra: Option<String> = q.get(2)?;
                Ok(json!({"kind": q.get::<_, String>(0)?, "score": q.get::<_, Option<f64>>(1)?,
                          "extra": extra.and_then(|e| serde_json::from_str::<Value>(&e).ok()), "ts": q.get::<_, i64>(3)?}))
            })
            .map_err(err)?
            .flatten()
            .collect();
        v
    };
    let (spo2, _) = crate::health_svc::spo2_series(conn);
    let temp = series(conn, "wrist_temp", "avg");
    Ok(json!({
        "date": date, "morning": morning, "evening": evening, "psych": psych,
        "vitals": vitals_json(date, &hd.hrv, &hd.rhr, &hd.resp, &spo2, &temp, false, &hd.cuts),
    }))
}

pub fn summary(conn: &mut Connection) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let p = profile(conn);
    let rest = resting_hr(conn, &p);
    let acts = refresh_metrics(conn, &p, rest)?;
    let today = today(&p);
    let tz = p.tz_offset_s;

    // Training load
    let daily = training::daily_load(&acts.iter().map(|a| (day_of(a.start_utc, tz), a.load)).collect::<Vec<_>>());
    let ls = training::load_series(&daily, &today);
    let cur = ls.last().cloned().unwrap_or_default();
    let (state, state_text) = training::load_state(cur.acwr);
    let series90: Vec<Value> = ls
        .iter()
        .rev()
        .take(90)
        .rev()
        .map(|d| json!([d.date, r1(d.load), r1(d.atl), r1(d.ctl), r1(d.tsb)]))
        .collect();

    // Intensity distribution over 7 and 28 days (workouts with pulse curves only).
    let split = |days: i64| {
        let from = add_days(&today, -days + 1);
        let zs: Vec<[f64; 6]> = acts
            .iter()
            .filter(|a| a.method == "hr" && day_of(a.start_utc, tz) >= from)
            .map(|a| a.zone_s)
            .collect();
        training::intensity_split(&zs).map(|(l, m, h, t)| json!({"low": r1(l * 100.0), "mid": r1(m * 100.0), "high": r1(h * 100.0), "hours": r1(t / 3600.0)}))
    };
    let zones28: [f64; 6] = {
        let from = add_days(&today, -27);
        let mut z = [0.0; 6];
        for a in acts.iter().filter(|a| a.method == "hr" && day_of(a.start_utc, tz) >= from) {
            for i in 0..6 {
                z[i] += a.zone_s[i];
            }
        }
        z
    };

    // Weeks (Monday start), last 12
    let mut weeks: std::collections::BTreeMap<String, (f64, f64, f64, i64)> = Default::default();
    for a in &acts {
        let w = training::week_start(&day_of(a.start_utc, tz));
        let e = weeks.entry(w).or_insert((0.0, 0.0, 0.0, 0));
        e.0 += a.load;
        e.1 += a.duration_s;
        if a.sport == "run" {
            e.2 += a.distance_m.unwrap_or(0.0);
        }
        e.3 += 1;
    }
    let weeks_v: Vec<Value> = weeks
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|(w, (l, s, km, n))| json!({"week": w, "load": r1(*l), "hours": r1(s / 3600.0), "runKm": r1(km / 1000.0), "count": n}))
        .collect();

    // Records and predictions
    let rec_in: Vec<(String, String, Vec<(f64, f64)>)> =
        acts.iter().filter(|a| !a.best.is_empty()).map(|a| (a.id.clone(), day_of(a.start_utc, tz), a.best.clone())).collect();
    let records: Vec<Value> = training::records(&rec_in)
        .iter()
        .map(|(d, (date, t, id))| json!({"distanceM": d, "timeS": t.round(), "date": date, "activityId": id}))
        .collect();
    let recent_from = add_days(&today, -120);
    let recent: Vec<(String, f64, f64)> = rec_in
        .iter()
        .filter(|(_, d, _)| *d >= recent_from)
        .flat_map(|(_, d, b)| b.iter().map(move |(dist, t)| (d.clone(), *dist, *t)))
        .collect();
    let preds: Vec<Value> = training::predictions(&recent)
        .iter()
        .map(|p| json!({"distanceM": p.distance_m, "timeS": p.time_s.round(), "fromDistanceM": p.from_distance_m, "fromTimeS": p.from_time_s.round(), "fromDate": p.from_date}))
        .collect();

    // Health series
    let HealthData { hrv, rhr, sleep_s, resp, breath, alcohol, nights, hrv_nightly, rhr_nightly, cuts } = health_data(conn);
    let h = HealthSeries { hrv: &hrv, rhr: &rhr, sleep_s: &sleep_s, resp: &resp, breath: &breath, alcohol: &alcohol, cuts: &cuts };
    let tsb_on = |d: &str| ls.iter().find(|l| l.date == d).map(|l| l.tsb);
    let mut ready = daily::readiness(&today, &h, tsb_on(&today));
    let mut comp: Vec<Value> = ready
        .components
        .iter()
        .map(|c| json!({"key": c.key, "label": c.label, "value": c.value, "baseline": c.baseline, "score": c.score.map(|s| s.round()), "weight": c.weight, "note": c.note}))
        .collect();
    let mut pen: Vec<Value> = ready.penalties.iter().map(|(t, v)| json!({"text": t, "points": v})).collect();
    // Morning readiness is frozen once the night is in, so daytime readings cannot rewrite it.
    let frozen: std::collections::HashMap<String, (Option<f64>, String, String)> = {
        let mut st = conn.prepare("SELECT date, score, level, data FROM readiness_log WHERE date >= ?1").map_err(err)?;
        let rows = st
            .query_map(params![add_days(&today, -130)], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, Option<f64>>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))))
            .map_err(err)?
            .flatten()
            .collect();
        rows
    };
    let slept = sleep_s.get(&today).map(|s| *s >= 3.0 * 3600.0).unwrap_or(false);
    let local_min = (store::now() + tz).rem_euclid(86400) / 60;
    if !frozen.contains_key(&today) && slept && ready.score.is_some() && ((hrv_nightly && hrv.contains_key(&today)) || local_min >= 15 * 60) {
        let data = json!({"components": comp, "penalties": pen}).to_string();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO readiness_log (date, score, level, data) VALUES (?1, ?2, ?3, ?4)",
            params![today, ready.score, ready.level, data],
        );
    } else if let Some((sc, lvl, data)) = frozen.get(&today) {
        ready.score = *sc;
        ready.level = match lvl.as_str() {
            "green" => "green",
            "yellow" => "yellow",
            "red" => "red",
            _ => "unknown",
        };
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(c) = v["components"].as_array() {
                comp = c.clone();
            }
            if let Some(p) = v["penalties"].as_array() {
                pen = p.clone();
            }
        }
    }
    let ready_hist: Vec<Value> = (0..120)
        .rev()
        .map(|k| {
            let d = add_days(&today, -k);
            let sc = match frozen.get(&d) {
                Some((sc, _, _)) => *sc,
                None if d == today => ready.score,
                None => daily::readiness(&d, &h, tsb_on(&d)).score,
            };
            json!([d, sc])
        })
        .collect();
    // Muscle load (separate track) and recovery time.
    let ml = crate::muscle_svc::loads(conn, &acts, &p);
    let mdaily = crate::muscle_svc::daily(&ml, tz);
    let (m_acwr, m_week, m_chronic) = crate::muscle_svc::acwr(&mdaily, &today);
    let recovery = crate::muscle_svc::recovery(&acts, &ml, &ls, &mdaily, tz, ready.score);
    crate::muscle_svc::store_recovery(conn, &recovery);
    let mut g7 = [0.0f64; 12];
    for m in ml.iter().filter(|m| day_of(m.start, tz) >= add_days(&today, -6)) {
        for i in 0..12 {
            g7[i] += m.groups[i];
        }
    }
    let groups7: Vec<Value> = crate::muscle_svc::GROUPS.iter().enumerate().map(|(i, (k, n))| json!({"key": k, "name": n, "value": g7[i].round()})).collect();
    let m_today = mdaily.get(&today).copied().unwrap_or(0.0);
    let m_acwr_r = m_acwr.map(|x| (x * 100.0).round() / 100.0);
    let muscle_v = json!({"today": r1(m_today), "week": m_week.round(), "usualWeek": m_chronic.round(), "acwr": m_acwr_r, "groups7": groups7});
    // History for the «Нагрузка: сердце и мышцы» card: one row a day for a year, and the workouts.
    let mut load_hist: Vec<Value> = Vec::new();
    for k in (0..365).rev() {
        let d = add_days(&today, -k);
        let (ma, _, mw) = crate::muscle_svc::acwr(&mdaily, &d);
        let l = ls.iter().find(|x| x.date == d);
        load_hist.push(json!([d, l.map(|x| r1(x.load)).unwrap_or(0.0), r1(mdaily.get(&d).copied().unwrap_or(0.0)),
                              l.and_then(|x| x.acwr).map(|x| (x * 100.0).round() / 100.0), ma.map(|x| (x * 100.0).round() / 100.0),
                              l.map(|x| (x.ctl * 7.0).round()), mw.round()]));
    }
    let year_from = store::now() - 366 * 86400;
    let act_names: std::collections::HashMap<String, Option<String>> = {
        let mut st = conn.prepare("SELECT id, name FROM va WHERE start_utc >= ?1").map_err(err)?;
        let v = st.query_map(params![year_from], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).map_err(err)?.flatten().collect();
        v
    };
    let muscle_map = crate::muscle_svc::map_state(&ml, &act_names);
    let heart_by: std::collections::HashMap<&str, f64> = acts.iter().map(|a| (a.id.as_str(), a.load)).collect();
    let load_acts: Vec<Value> = ml
        .iter()
        .filter(|m| m.start >= year_from)
        .map(|m| json!({"id": m.id, "start": m.start, "sport": m.sport, "name": act_names.get(&m.id).cloned().flatten(),
                        "heart": heart_by.get(m.id.as_str()).map(|x| x.round()), "muscle": m.total.round(), "up": m.up, "down": m.down, "bad": m.bad}))
        .collect();
    // Frozen-aware scores by day, for the week strip and the usual level.
    let rmap: std::collections::HashMap<String, Option<f64>> =
        ready_hist.iter().filter_map(|v| Some((v[0].as_str()?.to_string(), v[1].as_f64()))).collect();
    let nights_v: Vec<Value> = nights
        .iter()
        .rev()
        .take(14)
        .map(|n| json!({"date": n.date, "totalS": n.total_s.round(), "deepS": n.deep_s.round(), "remS": n.rem_s.round(), "coreS": n.core_s.round(), "awakeS": n.awake_s.round(), "score": n.score}))
        .collect();

    // Body
    let weight = series(conn, "body_mass", "median");
    let fat = series(conn, "body_fat_pct", "median");
    let lean = series(conn, "lean_body_mass", "median");
    let tw = daily::trend(&weight, &today);
    let tf = daily::trend(&fat, &today);
    let tl = daily::trend(&lean, &today);
    let tj = |t: &daily::Trend| {
        json!({"latest": t.latest.as_ref().map(|(d, v)| json!([d, r1(*v)])), "smooth": t.smooth.map(r1), "change7d": t.change_7d.map(r1), "change30d": t.change_30d.map(r1)})
    };

    // Nutrition
    let kcal = series(conn, "dietary_energy_total", "max");
    let prot = series(conn, "dietary_protein_total", "max");
    let carb = series(conn, "dietary_carbohydrates_total", "max");
    let fatg = series(conn, "dietary_fat_total_total", "max");
    let fib = series(conn, "dietary_fiber_total", "max");
    let water = series(conn, "dietary_water_total", "max");
    let w_now = tw.smooth.or_else(|| latest(&weight, &today, 60).map(|x| x.1)).unwrap_or(100.0);
    let year: i64 = today.get(0..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    let day_train = |d: &str| -> (f64, f64) {
        acts.iter()
            .filter(|a| day_of(a.start_utc, tz) == d)
            .fold((0.0, 0.0), |(k, m), a| (k + a.calories.unwrap_or(a.load * 4.0), m + a.duration_s / 60.0))
    };
    let food: Vec<Value> = (0..14)
        .rev()
        .map(|k| {
            let d = add_days(&today, -k);
            let (tk, tm) = day_train(&d);
            let t = daily::targets(&p, w_now, year, tk, tm);
            json!({
                "date": d, "kcal": kcal.get(&d), "protein": prot.get(&d), "carbs": carb.get(&d), "fat": fatg.get(&d),
                "fiber": fib.get(&d), "waterMl": water.get(&d),
                "target": {"kcal": t.energy_kcal, "protein": t.protein_g, "carbs": t.carbs_g, "fat": t.fat_g, "fiber": t.fiber_g, "waterMl": t.water_ml},
            })
        })
        .collect();
    let (tk, tm) = day_train(&today);
    let tt = daily::targets(&p, w_now, year, tk, tm);

    // Signals
    let zones_age = p.zones_date.as_deref().and_then(|d| training::days_since(d, &today));
    let over_max: Option<(usize, f64)> = {
        let since = store::now() - 180 * 86400;
        conn.query_row(
            "SELECT count(*), max(max_hr) FROM va WHERE sport = 'run' AND start_utc >= ?1 AND max_hr > ?2",
            params![since, p.hr_max + 2.0],
            |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, Option<f64>>(1)?.unwrap_or(0.0))),
        )
        .ok()
    };
    let sig = daily::signals(&daily::SignalInput {
        over_max,
        hr_max: p.hr_max,
        today: &today,
        rhr: &rhr,
        hrv: &hrv,
        sleep_s: &sleep_s,
        food_kcal: &kcal,
        acwr: cur.acwr,
        zones_age_days: zones_age,
        weight_change_30d: tw.change_30d,
        breath: &breath,
    });
    let mut sig_v: Vec<Value> = sig.iter().map(|s| json!({"level": s.level, "code": s.code, "text": s.text})).collect();
    // Chronic short sleep (0.17) — independent of the personal corridor.
    if let Some(t) = crate::health_svc::need_for(conn, &sleep_s, &hrv, &rhr, &today).chronic {
        sig_v.push(json!({"level": "warn", "code": "sleep_chronic", "text": t}));
    }

    // ----- the day: target load, recent workouts, this week, vitals, advice -----
    let usual_daily = cur.ctl.max(1.0);
    let drank = alcohol.get(&add_days(&today, -1)).map(|v| *v > 0.0).unwrap_or(false);
    // Range for today, with the top limited by how much the week can still take.
    let chronic_week = if ls.len() >= 28 {
        Some(ls[ls.len() - 28..].iter().map(|d| d.load).sum::<f64>() / 4.0)
    } else {
        None
    };
    let used6: f64 = ls.iter().rev().skip(1).take(6).map(|d| d.load).sum();
    // Safety gates for quality work and how you feel today (questionnaires only lower the plan).
    let yday = add_days(&today, -1);
    let quality_yesterday = acts
        .iter()
        .filter(|a| day_of(a.start_utc, tz) == yday)
        .any(|a| a.zone_s[4] + a.zone_s[5] >= 600.0 || a.zone_s[3] + a.zone_s[4] + a.zone_s[5] >= 1200.0);
    let body_warning = sig.iter().any(|g| g.code == "rhr_up" || g.code == "low_rhr_low_hrv");
    let surveys: Vec<(String, Option<f64>, Value, Value)> = {
        let mut st = conn.prepare("SELECT kind, score, answers, extra FROM surveys WHERE date = ?1 ORDER BY ts").map_err(err)?;
        let v = st
            .query_map(params![today], |q| {
                let a: String = q.get(2)?;
                let e: Option<String> = q.get(3)?;
                Ok((q.get::<_, String>(0)?, q.get::<_, Option<f64>>(1)?,
                    serde_json::from_str::<Value>(&a).unwrap_or(Value::Null),
                    e.and_then(|x| serde_json::from_str::<Value>(&x).ok()).unwrap_or(Value::Null)))
            })
            .map_err(err)?
            .flatten()
            .collect();
        v
    };
    let (feel, feel_reason) = {
        let mut lvl = 0u8;
        let mut why: Option<String> = None;
        let mut bump = |l: u8, w: String| {
            if l > lvl {
                lvl = l;
                why = Some(w);
            }
        };
        for (kind, score, answers, extra) in &surveys {
            match kind.as_str() {
                "tqr" => {
                    if let Some(t) = score {
                        let w = format!("по TQR ты восстановлен на {t:.0} из 20");
                        if *t <= 9.0 { bump(3, w) } else if *t <= 11.0 { bump(2, w) } else if *t <= 13.0 { bump(1, w) }
                    }
                }
                "hooper" => {
                    if let Some(f) = extra["feel"].as_f64() {
                        let w = format!("самочувствие по индексу Хупера {f:.0} из 100");
                        if f < 25.0 { bump(3, w) } else if f < 40.0 { bump(2, w) } else if f <= 50.0 { bump(1, w) }
                    }
                    let item = |i: usize| answers.as_array().and_then(|a| a.get(i)).and_then(|x| x.as_f64()).unwrap_or(0.0);
                    if item(2) >= 6.0 {
                        bump(2, "по индексу Хупера сильная усталость".into());
                    } else if item(3) >= 6.0 {
                        bump(2, "по индексу Хупера сильно болят мышцы".into());
                    }
                }
                "stressrec" => {
                    if let (Some(st), Some(rc)) = (extra["stress"].as_f64(), extra["recovery"].as_f64()) {
                        if st > rc {
                            bump(2, "в проверке стресса и восстановления стресс выше восстановления".into());
                        }
                    }
                }
                _ => {}
            }
        }
        (lvl, why)
    };
    let gates = daily::Gates {
        week_heavy: cur.acwr.map(|a| a > 1.3).unwrap_or(false) || m_acwr.map(|a| a > 1.3).unwrap_or(false),
        quality_yesterday,
        body_warning,
        feel,
        feel_reason,
        has_survey: surveys.iter().any(|x| x.0 == "tqr" || x.0 == "hooper"),
        drank,
    };
    let plan_m = daily::plan_day(ready.score, &p.zones, &gates);
    let tgt_m = daily::load_target(plan_m.lo, plan_m.hi, usual_daily, p.week_cap, chronic_week, used6);
    // Daily stress and evening readiness (morning score ± what the day did, at most ±15).
    let stress_all = crate::stress_svc::days(conn, &p, &add_days(&today, -31), &today);
    let st_today = stress_all.get(&today);
    let st_norm = crate::stress_svc::norm(&stress_all, &today);
    let tqr: Option<f64> = conn
        .query_row("SELECT score FROM surveys WHERE kind = 'tqr' AND date = ?1 ORDER BY ts DESC LIMIT 1", params![today], |r| r.get(0))
        .ok();
    let caffeine = series(conn, "dietary_caffeine_total", "max");
    let evening = crate::stress_svc::evening(&crate::stress_svc::EveningInput {
        morning: ready.score,
        load_done: cur.load,
        usual_day: usual_daily,
        target_hi: tgt_m.hi,
        kcal: kcal.get(&today).copied(),
        kcal_target: tt.energy_kcal,
        water: water.get(&today).copied(),
        water_target: tt.water_ml,
        alcohol: alcohol.get(&today).copied(),
        caffeine: caffeine.get(&today).copied(),
        stress: st_today,
        stress_norm: st_norm,
        tqr,
    });
    let eve_from = crate::stress_svc::evening_from(conn, &today, tz);
    if !evening.is_null() && local_min >= eve_from {
        let _ = conn.execute(
            "INSERT INTO evening_log (date, score, level, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(date) DO UPDATE SET score = excluded.score, level = excluded.level, data = excluded.data, saved_at = datetime('now')",
            params![today, evening["score"].as_f64().unwrap_or(0.0), evening["level"].as_str().unwrap_or("unknown"), evening.to_string()],
        );
    }
    let evening_hist: Vec<Value> = {
        let mut st = conn.prepare("SELECT date, score FROM evening_log WHERE date >= ?1 ORDER BY date").map_err(err)?;
        let v = st
            .query_map(params![add_days(&today, -400)], |q| Ok(json!([q.get::<_, String>(0)?, q.get::<_, f64>(1)?])))
            .map_err(err)?
            .flatten()
            .collect();
        v
    };
    let stress_v = json!({
        "today": st_today.map(|s| crate::stress_svc::day_json(s, tz, st_norm)),
        "history": stress_all.iter().map(|(d, s)| json!([d, s.score])).collect::<Vec<_>>(),
    });

    // In the evening the rest of the day is planned from the evening score.
    let eve_on = !evening.is_null() && local_min >= eve_from;
    let plan = if eve_on { daily::plan_day(evening["score"].as_f64(), &p.zones, &gates) } else { plan_m };
    let tgt = if eve_on { daily::load_target(plan.lo, plan.hi, usual_daily, p.week_cap, chronic_week, used6) } else { tgt_m };
    // Ways to fill the rest of the range, priced by your own past workouts (load per minute).
    let rate = |sport: &str| -> Option<f64> {
        let from = add_days(&today, -90);
        let v: Vec<f64> = acts
            .iter()
            .filter(|a| a.sport == sport && a.duration_s > 600.0 && day_of(a.start_utc, tz) >= from && a.method != "estimate")
            .map(|a| a.load / (a.duration_s / 60.0))
            .collect();
        median(&v)
    };
    let mid = (tgt.lo + tgt.hi) / 2.0;
    let left = (mid - cur.load).max(0.0);
    let mins = |r: f64| ((left / r / 5.0).round() * 5.0).clamp(15.0, 150.0);
    let mut options: Vec<Value> = Vec::new();
    if left < 10.0 {
        options.push(json!({"kind": "done", "title": "Норма дня набрана", "text": "Дальше — только восстановление: прогулка, растяжка, сон."}));
    } else {
        let z2 = p.zones[2] as i64;
        let z1 = p.zones[1] as i64;
        if plan.tier >= 3 {
            if let Some(r) = rate("run") {
                let what = match plan.tier {
                    6 => "Можно с интервалами по плану",
                    5 => "Можно с темповым отрезком",
                    _ => "Вторая зона",
                };
                options.push(json!({"kind": "run", "title": format!("Бег ≈ {:.0} мин", mins(r)), "text": format!("{what}, разминка и заминка до {z2}")}));
            }
        }
        if let Some(r) = rate("walk") {
            let m = if plan.tier <= 2 { mins(r).min(90.0) } else { mins(r) };
            options.push(json!({"kind": "walk", "title": format!("Ходьба ≈ {:.0} мин", m), "text": format!("Спокойный темп, пульс до {z1}")}));
        }
        if plan.tier >= 4 {
            if let Some(r) = rate("strength").or(rate("workout")) {
                options.push(json!({"kind": "strength", "title": format!("Силовая ≈ {:.0} мин", mins(r).min(75.0)), "text": "Можно вместе с лёгкой прогулкой"}));
            }
        }
        if plan.tier <= 2 {
            options.push(json!({"kind": "mobility", "title": "Мобилити и растяжка 20–30 мин", "text": "Почти без нагрузки — помогает восстановиться"}));
        }
    }
    let target_load = tgt.hi;
    let names: std::collections::HashMap<String, (Option<String>, Option<f64>)> = {
        let mut st = conn
            .prepare("SELECT id, name, relative_effort FROM va ORDER BY start_utc DESC LIMIT 60")
            .map_err(err)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, Option<String>>(1)?, r.get::<_, Option<f64>>(2)?))))
            .map_err(err)?
            .flatten()
            .collect();
        rows
    };
    let act_json = |a: &Cached| {
        let (name, re) = names.get(&a.id).cloned().unwrap_or((None, None));
        json!({"id": a.id, "sport": a.sport, "name": name, "startUtc": a.start_utc, "durationS": a.duration_s.round(),
               "distanceM": a.distance_m, "load": a.load.round(), "relativeEffort": re, "method": a.method})
    };
    let recent: Vec<Value> = acts.iter().rev().take(6).map(|a| act_json(a)).collect();
    let today_acts: Vec<Value> = acts.iter().filter(|a| day_of(a.start_utc, tz) == today).map(|a| act_json(a)).collect();
    let monday = training::week_start(&today);
    let week_days: Vec<Value> = (0..7)
        .map(|k| {
            let d = add_days(&monday, k);
            let list: Vec<&Cached> = acts.iter().filter(|a| day_of(a.start_utc, tz) == d).collect();
            json!({
                "date": d,
                "load": r1(list.iter().map(|a| a.load).sum::<f64>()),
                "sports": list.iter().map(|a| a.sport.clone()).collect::<Vec<_>>(),
                "minutes": (list.iter().map(|a| a.duration_s).sum::<f64>() / 60.0).round(),
                "readiness": if d <= today { rmap.get(&d).copied().flatten() } else { None },
            })
        })
        .collect();
    let usual_ready = {
        let v: Vec<f64> = (1..=14).filter_map(|k| rmap.get(&add_days(&today, -k)).copied().flatten()).collect();
        crate::engine::mean(&v).map(|x| x.round())
    };
    let (spo2, _) = crate::health_svc::spo2_series(conn);
    let temp = series(conn, "wrist_temp", "avg");
    let vitals = vitals_json(&today, &hrv, &rhr, &resp, &spo2, &temp, true, &cuts);
    let day = json!({
        "advice": {"title": plan.title, "text": plan.text, "tier": daily::TIERS[plan.tier], "bodyTier": daily::TIERS[plan.body_tier],
                   "reasons": plan.reasons, "hint": plan.hint, "evening": eve_on},
        "targetLoad": target_load,
        "target": {"lo": tgt.lo, "hi": tgt.hi, "headroom": tgt.headroom, "capped": tgt.capped, "cap": p.week_cap, "left": left.round()},
        "options": options,
        "usualLoad": usual_daily.round(),
        "todayLoad": r1(cur.load),
        "todayActivities": today_acts,
        "recent": recent,
        "week": week_days,
        "usualReadiness": usual_ready,
        "alcoholYesterday": alcohol.get(&add_days(&today, -1)).copied(),
        "vitals": vitals,
    });

    let methods = acts.iter().fold((0, 0, 0), |(h, a, e), x| match x.method.as_str() {
        "hr" => (h + 1, a, e),
        "avg_hr" => (h, a + 1, e),
        _ => (h, a, e + 1),
    });

    // New data sources whose norm is still being built (first 2 weeks after a switch).
    let calibrating: Vec<Value> = cuts
        .iter()
        .filter_map(|(g, c)| {
            let n = crate::engine::day_num(&today)? - crate::engine::day_num(c)?;
            let name = crate::sources::GROUPS.iter().find(|x| x.0 == g)?.1;
            (n < 14).then(|| json!({"group": g, "name": name, "from": c, "daysLeft": 14 - n, "provider": crate::sources::provider_name(&crate::sources::current(conn, g))}))
        })
        .collect();
    Ok(json!({
        "today": today,
        "profile": profile_json(&p),
        "hrRestUsed": rest,
        "readiness": {"score": ready.score, "level": ready.level, "components": comp,
            "penalties": pen, "history": ready_hist, "frozen": frozen.contains_key(&today), "calibrating": calibrating, "hrvNightly": hrv_nightly, "rhrNightly": rhr_nightly,
            "evening": evening, "eveningHistory": evening_hist, "eveningOn": eve_on,
            "eveningFrom": format!("{:02}:{:02}", eve_from / 60, eve_from % 60)},
        "stress": stress_v,
        "load": {"today": r1(cur.load), "atl": r1(cur.atl), "ctl": r1(cur.ctl), "tsb": r1(cur.tsb),
            "acwr": cur.acwr.map(|x| (x * 100.0).round() / 100.0), "week": r1(cur.week_sum), "state": state, "stateText": state_text,
            "series": series90, "muscle": muscle_v, "history": load_hist, "acts": load_acts},
        "recovery": recovery,
        "muscleMap": muscle_map,
        "intensity": {"d7": split(7), "d28": split(28), "zones28Min": zones28.iter().map(|s| (s / 60.0).round()).collect::<Vec<_>>()},
        "weeks": weeks_v,
        "records": records,
        "predictions": preds,
        "sleep": nights_v,
        "body": {"weight": tj(&tw), "fatPct": tj(&tf), "lean": tj(&tl)},
        "nutrition": {"days": food, "targetToday": {"kcal": tt.energy_kcal, "protein": tt.protein_g, "carbs": tt.carbs_g, "fat": tt.fat_g, "fiber": tt.fiber_g, "waterMl": tt.water_ml, "bmr": tt.bmr_kcal}, "weightUsed": r1(w_now)},
        "signals": sig_v,
        "day": day,
        "meta": {"activities": acts.len(), "byHr": methods.0, "byAvgHr": methods.1, "estimated": methods.2, "ms": started.elapsed().as_millis() as u64},
    }))
}
