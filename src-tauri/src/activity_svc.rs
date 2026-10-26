//! One workout in detail (charts, km splits, zones, sets), editing of felt
//! effort / notes / strength sets, and running shoes.

use crate::{db, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn arr_f(v: &Value, k: &str) -> Vec<Option<f64>> {
    v[k].as_array().map(|a| a.iter().map(|x| x.as_f64()).collect()).unwrap_or_default()
}

/// Kilometre splits from the distance stream: time, pace and average pulse per km.
fn splits(t: &[i64], dist: &[Option<f64>], hr: &[Option<f64>]) -> Vec<Value> {
    let mut out = Vec::new();
    let n = t.len().min(dist.len());
    if n < 2 {
        return out;
    }
    let mut next_km = 1000.0;
    let mut seg_start = 0usize;
    for i in 1..n {
        let Some(d) = dist[i] else { continue };
        if d >= next_km {
            let secs = (t[i] - t[seg_start]) as f64;
            let hrs: Vec<f64> = hr.get(seg_start..=i).unwrap_or(&[]).iter().flatten().copied().collect();
            let avg = if hrs.is_empty() { None } else { Some((hrs.iter().sum::<f64>() / hrs.len() as f64).round()) };
            out.push(json!({"km": (next_km / 1000.0) as i64, "timeS": secs, "hr": avg, "full": true}));
            seg_start = i;
            next_km += 1000.0;
        }
    }
    // The last partial kilometre, if it is meaningful.
    if let Some(Some(last)) = dist.get(n - 1) {
        let rest = last - (next_km - 1000.0);
        if rest >= 200.0 {
            let secs = (t[n - 1] - t[seg_start]) as f64;
            let hrs: Vec<f64> = hr.get(seg_start..n).unwrap_or(&[]).iter().flatten().copied().collect();
            let avg = if hrs.is_empty() { None } else { Some((hrs.iter().sum::<f64>() / hrs.len() as f64).round()) };
            out.push(json!({"km": (next_km / 1000.0) as i64, "timeS": secs, "hr": avg, "full": false, "distM": rest.round()}));
        }
    }
    out
}

pub fn detail(conn: &Connection, id: &str) -> Result<Value, String> {
    let a = store::activity_row(conn, id)?;
    #[allow(clippy::type_complexity)]
    let (description, notes, weight_lifted, gear, shoe_id): (Option<String>, Option<String>, Option<f64>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT description, notes, weight_lifted_kg, gear, shoe_id FROM activities WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(err)?;
    let metrics: Option<(f64, String, String)> = conn
        .query_row(
            "SELECT load, method, zone_s FROM activity_metrics WHERE activity_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    // Streams, thinned to about 1500 points for the charts.
    let mut charts = Value::Null;
    let mut km = Vec::new();
    if let Some(js) = store::streams_json(conn, id) {
        let v: Value = serde_json::from_str(&js).unwrap_or(Value::Null);
        let t: Vec<i64> = v["t"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
        let hr = arr_f(&v, "hr");
        let dist = arr_f(&v, "dist");
        let speed = arr_f(&v, "speed");
        let alt = arr_f(&v, "alt");
        let cad = arr_f(&v, "cad");
        let lat = arr_f(&v, "lat");
        let lng = arr_f(&v, "lng");
        km = splits(&t, &dist, &hr);
        let step = (t.len() / 1500).max(1);
        let pick = |s: &Vec<Option<f64>>| -> Vec<Option<f64>> { s.iter().step_by(step).copied().collect() };
        charts = json!({
            "t": t.iter().step_by(step).copied().collect::<Vec<_>>(),
            "hr": pick(&hr), "dist": pick(&dist), "speed": pick(&speed), "alt": pick(&alt), "cad": pick(&cad),
            "lat": pick(&lat), "lng": pick(&lng),
        });
    }

    let mut st = conn
        .prepare(
            "SELECT idx, start_utc, duration_s, reps, weight_kg, kind, exercise, category, ex_id, band, eff_kg FROM strength_sets
             WHERE activity_id = ?1 AND cx IS NULL ORDER BY idx",
        )
        .map_err(err)?;
    let sets: Vec<Value> = st
        .query_map(params![id], |r| {
            Ok(json!({
                "idx": r.get::<_, i64>(0)?, "startUtc": r.get::<_, Option<i64>>(1)?, "durationS": r.get::<_, Option<f64>>(2)?,
                "reps": r.get::<_, Option<i64>>(3)?, "weightKg": r.get::<_, Option<f64>>(4)?, "kind": r.get::<_, Option<String>>(5)?,
                "exercise": r.get::<_, Option<String>>(6)?, "category": r.get::<_, Option<i64>>(7)?,
                "exId": r.get::<_, Option<String>>(8)?,
                "band": r.get::<_, Option<String>>(9)?.and_then(|t| serde_json::from_str::<Value>(&t).ok()),
                "effKg": r.get::<_, Option<f64>>(10)?,
            }))
        })
        .map_err(err)?
        .flatten()
        .collect();

    let muscle = crate::muscle_svc::for_activity(conn, id);
    let (src, dev, hs, hg, hidden, dstate): (String, Option<String>, i64, i64, i64, Option<String>) = conn
        .query_row("SELECT source, device, has_streams, has_gps, hidden, dup_state FROM activities WHERE id = ?1", params![id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .map_err(err)?;
    let alternatives = crate::workouts_svc::alternatives(conn, id);
    let source_name = crate::workouts_svc::source_name(&src, dev.as_deref());
    let detail_text = crate::workouts_svc::detail_text(hs, hg, &src);
    Ok(json!({
        "sourceName": source_name, "detailText": detail_text, "hidden": hidden, "dupState": dstate, "alternatives": alternatives,
        "activity": a,
        "description": description,
        "notes": notes,
        "weightLiftedKg": weight_lifted,
        "gear": gear,
        "shoeId": shoe_id,
        "load": metrics.as_ref().map(|m| m.0.round()),
        "loadMethod": metrics.as_ref().map(|m| m.1.clone()),
        "zoneS": metrics.as_ref().and_then(|m| serde_json::from_str::<Value>(&m.2).ok()),
        "charts": charts,
        "splits": km,
        "sets": sets,
        "bodyKg": crate::exercise_svc::activity_date(conn, id).and_then(|d| crate::exercise_svc::body_kg(conn, &d)).map(|x| (x * 10.0).round() / 10.0),
        "muscle": muscle,
        "ofp": ofp_blocks(conn, id),
    }))
}

pub fn update(conn: &Connection, id: &str, rpe: Option<f64>, notes: Option<String>) -> Result<(), String> {
    let rpe = rpe.filter(|r| (1.0..=10.0).contains(r));
    let n = conn
        .execute(
            "UPDATE activities SET rpe = ?2, notes = ?3, updated_at = datetime('now') WHERE id = ?1",
            params![id, rpe, notes.filter(|s| !s.trim().is_empty())],
        )
        .map_err(err)?;
    if n == 0 {
        return Err("Тренировка не найдена".into());
    }
    Ok(())
}

pub fn save_sets(conn: &mut Connection, id: &str, sets: &[Value]) -> Result<(), String> {
    let body = crate::exercise_svc::activity_date(conn, id).and_then(|d| crate::exercise_svc::body_kg(conn, &d));
    let bands = crate::exercise_svc::bands(conn);
    let tx = conn.transaction().map_err(err)?;
    // 1.4: rows of the «ОФП и комплексы» card are kept; they are saved by `save_ofp`.
    tx.execute("DELETE FROM strength_sets WHERE activity_id = ?1 AND cx IS NULL", params![id]).map_err(err)?;
    for (i, s) in sets.iter().enumerate() {
        insert_set(&tx, id, i as i64, s, None, body, &bands)?;
    }
    recount(&tx, id)?;
    tx.commit().map_err(err)
}

fn insert_set(tx: &rusqlite::Transaction, id: &str, idx: i64, s: &Value, cx: Option<String>, body: Option<f64>, bands: &[Value]) -> Result<(), String> {
    let reps = s["reps"].as_i64().filter(|r| *r >= 0);
    let w = s["weightKg"].as_f64().filter(|w| *w >= 0.0);
    let kind = s["kind"].as_str().unwrap_or("active").to_string();
    let ex = s["exercise"].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let ex_id = s["exId"].as_str().filter(|x| !x.is_empty() && crate::exercise_svc::by_id(x).is_some()).map(String::from);
    let band = s["band"].as_object().filter(|b| b.get("ids").and_then(|x| x.as_array()).map(|a| !a.is_empty()).unwrap_or(false)).map(|b| Value::Object(b.clone()).to_string());
    let (eff, _, _) = crate::exercise_svc::effective(s, body, bands);
    if let Some(name) = ex.as_ref().filter(|_| cx.is_none()) {
        tx.execute("INSERT OR IGNORE INTO exercises (name, source) VALUES (?1, 'manual')", params![name]).map_err(err)?;
    }
    tx.execute(
        "INSERT INTO strength_sets (activity_id, idx, start_utc, duration_s, reps, weight_kg, kind, exercise, category, ex_id, band, eff_kg, cx)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![id, idx, s["startUtc"].as_i64(), s["durationS"].as_f64(), reps, w, kind, ex, s["category"].as_i64(), ex_id, band, eff, cx],
    )
    .map_err(err)?;
    Ok(())
}

/// Sets, reps and tonnage of the workout from all its rows (ordinary sets and ОФП blocks).
fn recount(tx: &rusqlite::Transaction, id: &str) -> Result<(), String> {
    let (n_sets, n_reps, lifted): (i64, i64, f64) = tx
        .query_row(
            "SELECT count(*), COALESCE(sum(reps), 0), COALESCE(sum(COALESCE(eff_kg, weight_kg, 0) * COALESCE(reps, 0)), 0)
             FROM strength_sets WHERE activity_id = ?1 AND COALESCE(kind, 'active') = 'active'",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    tx.execute(
        "UPDATE activities SET total_sets = ?2, total_reps = ?3, weight_lifted_kg = ?4, updated_at = datetime('now') WHERE id = ?1",
        params![id, if n_sets > 0 { Some(n_sets) } else { None }, if n_sets > 0 { Some(n_reps) } else { None }, if lifted > 0.0 { Some(lifted) } else { None }],
    )
    .map_err(err)?;
    Ok(())
}

// ---------- ОФП and complexes (1.4) ----------

/// Saves the blocks of the «ОФП и комплексы» card: each block (a complex or one exercise) is
/// expanded into ordinary set rows — rounds × exercises — so tonnage, muscle load and the map
/// work as for any sets. Block: {cid, name, rounds, items: [{exId, name, reps, sec, leg,
/// load: {kind none|band|kg, kg, band: {ids, mode}}, skip}]}. A skipped exercise («не делал»)
/// keeps one row of kind `skip` so the block looks the same next time.
pub fn save_ofp(conn: &mut Connection, id: &str, blocks: &[Value]) -> Result<(), String> {
    let body = crate::exercise_svc::activity_date(conn, id).and_then(|d| crate::exercise_svc::body_kg(conn, &d));
    let bands = crate::exercise_svc::bands(conn);
    let tx = conn.transaction().map_err(err)?;
    tx.execute("DELETE FROM strength_sets WHERE activity_id = ?1 AND cx IS NOT NULL", params![id]).map_err(err)?;
    let mut idx = 10_000i64;
    for (b, bl) in blocks.iter().enumerate() {
        let rounds = bl["rounds"].as_i64().unwrap_or(1).clamp(1, 30);
        let items = bl["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        let cid = bl["cid"].as_str().filter(|x| !x.is_empty());
        let name = bl["name"].as_str().unwrap_or("").trim();
        for r in 0..=rounds {
            for (i, it) in items.iter().enumerate() {
                let skip = it["skip"].as_bool().unwrap_or(false);
                // Round 0 holds only the skipped exercises.
                if (r == 0) != skip {
                    continue;
                }
                let load = &it["load"];
                let set = json!({
                    "exId": it["exId"], "exercise": it["name"], "kind": if skip { "skip" } else { "active" },
                    "reps": if it["sec"].as_f64().is_some() { Value::Null } else { it["reps"].clone() },
                    "durationS": it["sec"],
                    "weightKg": if load["kind"] == "kg" { load["kg"].clone() } else { Value::Null },
                    "band": if load["kind"] == "band" { load["band"].clone() } else { Value::Null },
                });
                let cx = json!({"c": cid, "n": name, "b": b, "r": r, "s": rounds, "i": i, "leg": it["leg"].as_bool().unwrap_or(false)}).to_string();
                insert_set(&tx, id, idx, &set, Some(cx), body, &bands)?;
                idx += 1;
            }
        }
    }
    recount(&tx, id)?;
    tx.commit().map_err(err)
}

type CxRow = (i64, Value, Option<String>, Option<String>, Option<i64>, Option<f64>, Option<f64>, Option<String>, String);

/// Blocks back from the set rows: (activity, block) → block JSON, in order.
fn blocks_from(rows: &[CxRow]) -> Vec<(i64, Value)> {
    let mut out: Vec<(i64, i64, Value)> = Vec::new();
    for (start, cx, ex_id, name, reps, sec, kg, band, kind) in rows {
        let b = cx["b"].as_i64().unwrap_or(0);
        let r = cx["r"].as_i64().unwrap_or(1);
        let i = cx["i"].as_i64().unwrap_or(0) as usize;
        let pos = match out.iter().position(|x| x.0 == *start && x.1 == b) {
            Some(p) => p,
            None => {
                out.push((*start, b, json!({"cid": cx["c"], "name": cx["n"], "rounds": cx["s"].as_i64().unwrap_or(1), "items": []})));
                out.len() - 1
            }
        };
        // The first round (or the skip row) describes the exercise.
        if !(r == 1 || kind == "skip") {
            continue;
        }
        let band_v = band.as_deref().and_then(|t| serde_json::from_str::<Value>(t).ok());
        let load = if band_v.is_some() { json!({"kind": "band", "band": band_v}) } else if kg.is_some() { json!({"kind": "kg", "kg": kg}) } else { json!({"kind": "none"}) };
        let items = out[pos].2["items"].as_array_mut().unwrap();
        while items.len() <= i {
            items.push(Value::Null);
        }
        items[i] = json!({"exId": ex_id, "name": name, "reps": reps, "sec": sec, "leg": cx["leg"].as_bool().unwrap_or(false), "load": load, "skip": kind == "skip"});
    }
    out.into_iter()
        .map(|(s, _, mut v)| {
            if let Some(a) = v["items"].as_array_mut() {
                a.retain(|x| !x.is_null());
            }
            (s, v)
        })
        .collect()
}

fn cx_rows(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Vec<CxRow> {
    let Ok(mut st) = conn.prepare(sql) else { return Vec::new() };
    let v: Vec<CxRow> = st
        .query_map(p, |r| {
            let cx: String = r.get(1)?;
            Ok((r.get(0)?, serde_json::from_str(&cx).unwrap_or(Value::Null), r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get::<_, Option<String>>(8)?.unwrap_or_else(|| "active".into())))
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    v
}

/// The ОФП blocks of one workout.
pub fn ofp_blocks(conn: &Connection, id: &str) -> Vec<Value> {
    let rows = cx_rows(
        conn,
        "SELECT 0, cx, ex_id, exercise, reps, duration_s, weight_kg, band, kind FROM strength_sets WHERE activity_id = ?1 AND cx IS NOT NULL ORDER BY idx",
        &[&id],
    );
    blocks_from(&rows).into_iter().map(|x| x.1).collect()
}

/// «В прошлый раз»: for every complex (key = its id) and single exercise (key `ex:<id>`) the
/// latest earlier workout where it was done, with the date. Nothing is filled in by itself —
/// the card shows it as a hint with a button.
pub fn ofp_last(conn: &Connection, id: &str) -> Value {
    let start: i64 = conn.query_row("SELECT start_utc FROM activities WHERE id = ?1", params![id], |r| r.get(0)).unwrap_or(i64::MAX);
    let tz = crate::engine_svc::profile(conn).tz_offset_s;
    let rows = cx_rows(
        conn,
        "SELECT a.start_utc, s.cx, s.ex_id, s.exercise, s.reps, s.duration_s, s.weight_kg, s.band, s.kind FROM strength_sets s
         JOIN va a ON a.id = s.activity_id
         WHERE s.cx IS NOT NULL AND a.start_utc < ?1 AND a.start_utc >= ?1 - 400 * 86400 ORDER BY a.start_utc DESC, s.idx",
        &[&start],
    );
    let mut out = serde_json::Map::new();
    for (st, b) in blocks_from(&rows) {
        let key = match b["cid"].as_str() {
            Some(c) => c.to_string(),
            None => match b["items"][0]["exId"].as_str() {
                Some(e) => format!("ex:{e}"),
                None => format!("name:{}", b["items"][0]["name"].as_str().unwrap_or("")),
            },
        };
        out.entry(key).or_insert_with(|| json!({"date": crate::parse::time::ymd(st, tz), "block": b}));
    }
    Value::Object(out)
}

pub fn exercises(conn: &Connection) -> Vec<String> {
    let Ok(mut st) = conn.prepare(
        "SELECT name FROM exercises UNION SELECT DISTINCT exercise FROM strength_sets WHERE exercise IS NOT NULL ORDER BY 1",
    ) else {
        return Vec::new();
    };
    let out = st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()).unwrap_or_default();
    out
}

// ---------- shoes ----------

/// Shoes are kept as JSON in settings: [{id, name, kind, startDate, initialKm, limitKm, walks, retired, main}].
/// 0.23.1 (owner): a workout counts only for the shoes the athlete picked for it — nothing is added
/// on its own; earlier mileage is the «пробег до этой даты» he enters himself.
pub fn shoes(conn: &Connection, _tz: i64) -> Result<Value, String> {
    let raw = db::get_setting(conn, "shoes").unwrap_or_else(|| "[]".into());
    let list: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
    let mut out = Vec::new();
    for s in &list {
        let id = s["id"].as_str().unwrap_or("");
        let (km_a, n_a): (f64, i64) = conn
            .query_row(
                "SELECT COALESCE(sum(distance_m), 0) / 1000.0, count(*) FROM va WHERE shoe_id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        let initial = s["initialKm"].as_f64().unwrap_or(0.0);
        let mut v = s.clone();
        v["km"] = json!(((km_a + initial) * 10.0).round() / 10.0);
        v["workouts"] = json!(n_a);
        v["main"] = json!(s["main"].as_bool().unwrap_or(false));
        out.push(v);
    }
    Ok(Value::Array(out))
}

/// Workouts counted for one pair (newest first), to check them and fix a wrong pick.
pub fn shoe_runs(conn: &Connection, id: &str) -> Result<Value, String> {
    let mut st = conn
        .prepare("SELECT id, sport, name, start_utc, distance_m FROM va WHERE shoe_id = ?1 ORDER BY start_utc DESC LIMIT 300")
        .map_err(err)?;
    let v: Vec<Value> = st
        .query_map(params![id], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?,
                      "startUtc": r.get::<_, i64>(3)?, "distanceM": r.get::<_, Option<f64>>(4)?}))
        })
        .map_err(err)?
        .flatten()
        .collect();
    Ok(Value::Array(v))
}

/// Should Kouros ask for the shoes of this sport? Runs always (when pairs exist), walks/hikes only
/// when some active pair is worn for walking too.
pub fn ask_shoe(conn: &Connection, sport: &str) -> bool {
    let list: Vec<Value> = serde_json::from_str(&db::get_setting(conn, "shoes").unwrap_or_else(|| "[]".into())).unwrap_or_default();
    let active: Vec<&Value> = list.iter().filter(|s| !s["retired"].as_bool().unwrap_or(false)).collect();
    if active.is_empty() {
        return false;
    }
    sport.contains("run") || (matches!(sport, "walk" | "hike") && active.iter().any(|s| s["walks"].as_bool().unwrap_or(false)))
}

/// `shoe`: a shoe id, "none" (answered: no listed shoes), or null (not answered).
pub fn set_shoe(conn: &Connection, id: &str, shoe: Option<String>) -> Result<(), String> {
    conn.execute("UPDATE activities SET shoe_id = ?2 WHERE id = ?1", params![id, shoe.filter(|s| !s.is_empty())]).map_err(err)?;
    Ok(())
}

pub fn save_shoes(conn: &Connection, list: &Value) -> Result<(), String> {
    let arr = list.as_array().ok_or("Ожидался список")?;
    let clean: Vec<Value> = arr
        .iter()
        .map(|s| {
            json!({
                "id": s["id"].as_str().unwrap_or(""), "name": s["name"].as_str().unwrap_or("Кроссовки"),
                "kind": s["kind"].as_str().unwrap_or("train"), "startDate": s["startDate"].as_str().unwrap_or("2026-01-01"),
                "initialKm": s["initialKm"].as_f64().unwrap_or(0.0), "limitKm": s["limitKm"].as_f64().unwrap_or(700.0),
                "walks": s["walks"].as_bool().unwrap_or(false), "retired": s["retired"].as_bool().unwrap_or(false),
                "main": s["main"].as_bool().unwrap_or(false),
                // 0.23: characteristics (shoe_svc) and a link to the model's review page.
                "spec": s["spec"].as_object().map(|o| Value::Object(o.clone())).unwrap_or_else(|| json!({})),
                "url": s["url"].as_str().filter(|u| u.starts_with("http")).unwrap_or(""),
            })
        })
        .collect();
    db::set_setting(conn, "shoes", &Value::Array(clean).to_string())
}
