//! Training tab extras: running tolerance, weekly history, weekly intensity,
//! the "which shoes?" question and records beyond running bests.

use crate::db;
use crate::engine::training::week_start;
use crate::engine::{add_days, day_of, median};
use crate::engine_svc::{self, Cached};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Sport groups used by the history chart.
fn group(sport: &str) -> &'static str {
    match sport {
        "run" => "run",
        "walk" | "hike" => "walk",
        "strength" => "strength",
        "workout" | "yoga" => "workout",
        "ride" | "ride_virtual" => "ride",
        _ => "other",
    }
}

pub fn overview(conn: &mut Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let rest = engine_svc::resting_hr(conn, &p);
    let acts: Vec<Cached> = engine_svc::refresh_metrics(conn, &p, rest)?;
    let tz = p.tz_offset_s;
    let today = engine_svc::today(&p);
    let elev: HashMap<String, f64> = {
        let mut st = conn.prepare("SELECT id, elev_gain_m FROM va WHERE elev_gain_m IS NOT NULL").map_err(err)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))).map_err(err)?.flatten().collect();
        rows
    };
    let day = |a: &Cached| day_of(a.start_utc, tz);

    // ----- running tolerance: "equivalent km" = distance + 1 km per 100 m climb, scaled by intensity -----
    let from90 = add_days(&today, -90);
    let per_km: Vec<f64> = acts
        .iter()
        .filter(|a| a.sport == "run" && day(a) >= from90 && a.distance_m.unwrap_or(0.0) > 1000.0 && a.method != "estimate")
        .map(|a| a.load / (a.distance_m.unwrap_or(1.0) / 1000.0))
        .collect();
    let ref_per_km = median(&per_km);
    let eq = |a: &Cached| -> (f64, f64) {
        // (plain km, extra for climbing and intensity)
        let km = a.distance_m.unwrap_or(0.0) / 1000.0;
        let climb = elev.get(&a.id).copied().unwrap_or(0.0) / 100.0;
        let f = match ref_per_km {
            Some(rf) if km > 0.5 && rf > 0.0 && a.method != "estimate" => ((a.load / km) / rf).sqrt().clamp(0.85, 1.25),
            _ => 1.0,
        };
        let total = (km + climb) * f;
        (km, (total - km).max(0.0))
    };
    let mut days7 = Vec::new();
    for k in (0..7).rev() {
        let d = add_days(&today, -k);
        let (mut km, mut extra, mut walk) = (0.0, 0.0, 0.0);
        for a in acts.iter().filter(|a| day(a) == d) {
            match group(&a.sport) {
                "run" => {
                    let (k1, e1) = eq(a);
                    km += k1;
                    extra += e1;
                }
                "walk" => walk += (a.distance_m.unwrap_or(0.0) / 1000.0 + elev.get(&a.id).copied().unwrap_or(0.0) / 100.0) * 0.3,
                _ => {}
            }
        }
        days7.push(json!({"date": d, "km": r1(km), "extra": r1(extra), "walk": r1(walk)}));
    }
    let (from28, to28) = (add_days(&today, -34), add_days(&today, -7));
    let (mut run28, mut walk28) = (0.0, 0.0);
    for a in acts.iter().filter(|a| {
        let d = day(a);
        d >= from28 && d <= to28
    }) {
        match group(&a.sport) {
            "run" => {
                let (k1, e1) = eq(a);
                run28 += k1 + e1;
            }
            "walk" => walk28 += (a.distance_m.unwrap_or(0.0) / 1000.0 + elev.get(&a.id).copied().unwrap_or(0.0) / 100.0) * 0.3,
            _ => {}
        }
    }
    let tolerance = json!({"days": days7, "avgWeekRun": r1(run28 / 4.0), "avgWeekWalk": r1(walk28 / 4.0)});

    // ----- history by week, 52 weeks -----
    let first_week = week_start(&add_days(&today, -7 * 51));
    let mut weeks: BTreeMap<String, BTreeMap<&'static str, [f64; 5]>> = BTreeMap::new();
    let mut k = 0;
    loop {
        let w = add_days(&first_week, 7 * k);
        if w > today {
            break;
        }
        weeks.insert(w, BTreeMap::new());
        k += 1;
    }
    for a in acts.iter().filter(|a| day(a) >= first_week) {
        let w = week_start(&day(a));
        if let Some(m) = weeks.get_mut(&w) {
            let e = m.entry(group(&a.sport)).or_insert([0.0; 5]);
            e[0] += 1.0;
            e[1] += a.duration_s;
            e[2] += a.distance_m.unwrap_or(0.0);
            e[3] += elev.get(&a.id).copied().unwrap_or(0.0);
            e[4] += a.load;
        }
    }
    let history: Vec<Value> = weeks
        .iter()
        .map(|(w, m)| {
            let s: serde_json::Map<String, Value> = m
                .iter()
                .map(|(g, v)| (g.to_string(), json!([v[0], v[1].round(), v[2].round(), v[3].round(), v[4].round()])))
                .collect();
            json!({"week": w, "s": s})
        })
        .collect();
    let year_from = add_days(&today, -365);
    let mut sports_year: Vec<&str> = Vec::new();
    for g in ["run", "walk", "strength", "workout", "ride", "other"] {
        if acts.iter().any(|a| group(&a.sport) == g && day(a) >= year_from) {
            sports_year.push(g);
        }
    }

    // ----- intensity by week (pulse-based workouts only), 8 weeks -----
    let iw_from = week_start(&add_days(&today, -7 * 7));
    let mut iw: BTreeMap<String, ([f64; 6], [f64; 6], i64)> = BTreeMap::new();
    for k in 0..8 {
        let w = add_days(&iw_from, 7 * k);
        if w <= today {
            iw.insert(w, ([0.0; 6], [0.0; 6], 0));
        }
    }
    for a in acts.iter().filter(|a| a.method == "hr" && day(a) >= iw_from) {
        if let Some(e) = iw.get_mut(&week_start(&day(a))) {
            for i in 0..6 {
                e.1[i] += a.zone_s[i];
                if a.sport == "run" {
                    e.0[i] += a.zone_s[i];
                }
            }
            if a.sport == "run" {
                e.2 += 1;
            }
        }
    }
    let intensity_weeks: Vec<Value> = iw
        .iter()
        .map(|(w, (run, all, n))| {
            json!({"week": w, "run": run.iter().map(|s| (s / 60.0).round()).collect::<Vec<_>>(),
                   "all": all.iter().map(|s| (s / 60.0).round()).collect::<Vec<_>>(), "runs": n})
        })
        .collect();

    // ----- which shoes? the latest run (or walk, when a pair is worn for walking) of 14 days without an answer -----
    let ask = conn
        .prepare(
            "SELECT id, sport, name, start_utc, distance_m FROM va
             WHERE sport IN ('run', 'walk', 'hike') AND shoe_id IS NULL AND start_utc >= ?1
             ORDER BY start_utc DESC LIMIT 20",
        )
        .ok()
        .and_then(|mut st| {
            let rows: Vec<Value> = st
                .query_map(params![crate::store::now() - 14 * 86400], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?,
                              "startUtc": r.get::<_, i64>(3)?, "distanceM": r.get::<_, Option<f64>>(4)?}))
                })
                .ok()?
                .flatten()
                .collect();
            rows.into_iter().find(|a| crate::activity_svc::ask_shoe(conn, a["sport"].as_str().unwrap_or("")))
        })
        .unwrap_or(Value::Null);

    Ok(json!({
        "tolerance": tolerance,
        "history": history,
        "sportsYear": sports_year,
        "intensityWeeks": intensity_weeks,
        "shoeQuestion": ask,
    }))
}

// ---------- records beyond running bests ----------

fn epley(w: f64, reps: f64) -> f64 {
    w * (1.0 + reps / 30.0)
}

pub fn records(conn: &Connection) -> Result<Value, String> {
    // Best estimated one-rep max per exercise from saved sets (1–12 reps are trustworthy for the formula).
    let mut best: BTreeMap<String, Value> = BTreeMap::new();
    {
        let mut st = conn
            .prepare(
                "SELECT s.exercise, s.weight_kg, s.reps, a.start_utc, a.id FROM strength_sets s
                 JOIN va a ON a.id = s.activity_id
                 WHERE s.kind = 'active' AND s.exercise IS NOT NULL AND s.weight_kg > 0 AND s.reps BETWEEN 1 AND 12",
            )
            .map_err(err)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?)))
            .map_err(err)?;
        for (ex, w, reps, t, id) in rows.flatten() {
            let orm = epley(w, reps as f64);
            if best.get(&ex).and_then(|v| v["orm"].as_f64()).map(|b| orm > b).unwrap_or(true) {
                best.insert(ex.clone(), json!({"exercise": ex, "orm": r1(orm), "weightKg": w, "reps": reps, "startUtc": t, "activityId": id, "manual": false}));
            }
        }
    }
    let manual: Vec<Value> = serde_json::from_str(&db::get_setting(conn, "strength_manual").unwrap_or_else(|| "[]".into())).unwrap_or_default();
    for m in &manual {
        let (Some(ex), Some(w), Some(reps)) = (m["exercise"].as_str(), m["weightKg"].as_f64(), m["reps"].as_f64()) else { continue };
        let orm = epley(w, reps);
        if best.get(ex).and_then(|v| v["orm"].as_f64()).map(|b| orm > b).unwrap_or(true) {
            best.insert(ex.to_string(), json!({"exercise": ex, "orm": r1(orm), "weightKg": w, "reps": reps, "date": m["date"], "activityId": null, "manual": true}));
        }
    }
    let longest = |sport_sql: &str, order: &str| -> Value {
        conn.query_row(
            &format!("SELECT id, distance_m, COALESCE(moving_s, elapsed_s), start_utc FROM va WHERE {sport_sql} ORDER BY {order} DESC LIMIT 1"),
            [],
            |r| Ok(json!({"activityId": r.get::<_, String>(0)?, "distanceM": r.get::<_, Option<f64>>(1)?, "timeS": r.get::<_, Option<f64>>(2)?, "startUtc": r.get::<_, i64>(3)?})),
        )
        .unwrap_or(Value::Null)
    };
    let hidden: Vec<String> = serde_json::from_str(&db::get_setting(conn, "records_hidden").unwrap_or_else(|| "[]".into())).unwrap_or_default();
    Ok(json!({
        "strength": best.into_values().collect::<Vec<_>>(),
        "manual": manual,
        "longestRun": longest("sport = 'run' AND distance_m IS NOT NULL", "distance_m"),
        "longestWalk": longest("sport IN ('walk', 'hike') AND distance_m IS NOT NULL", "distance_m"),
        "longestSession": longest("1 = 1", "COALESCE(moving_s, elapsed_s)"),
        "hidden": hidden,
    }))
}

pub fn set_list(conn: &Connection, key: &str, v: &Value) -> Result<(), String> {
    if !v.is_array() {
        return Err("Ожидался список".into());
    }
    db::set_setting(conn, key, &v.to_string())
}
