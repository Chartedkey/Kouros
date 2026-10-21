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
             WHERE activity_id = ?1 ORDER BY idx",
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
    tx.execute("DELETE FROM strength_sets WHERE activity_id = ?1", params![id]).map_err(err)?;
    let (mut n_sets, mut n_reps, mut lifted) = (0i64, 0i64, 0.0f64);
    for (i, s) in sets.iter().enumerate() {
        let reps = s["reps"].as_i64().filter(|r| *r >= 0);
        let w = s["weightKg"].as_f64().filter(|w| *w >= 0.0);
        let kind = s["kind"].as_str().unwrap_or("active").to_string();
        let ex = s["exercise"].as_str().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
        let ex_id = s["exId"].as_str().filter(|x| !x.is_empty() && crate::exercise_svc::by_id(x).is_some()).map(String::from);
        let band = s["band"].as_object().filter(|b| b.get("ids").and_then(|x| x.as_array()).map(|a| !a.is_empty()).unwrap_or(false)).map(|b| Value::Object(b.clone()).to_string());
        let (eff, _, _) = crate::exercise_svc::effective(s, body, &bands);
        if kind == "active" {
            n_sets += 1;
            n_reps += reps.unwrap_or(0);
            lifted += eff.or(w).unwrap_or(0.0) * reps.unwrap_or(0) as f64;
        }
        if let Some(name) = &ex {
            tx.execute("INSERT OR IGNORE INTO exercises (name, source) VALUES (?1, 'manual')", params![name]).map_err(err)?;
        }
        tx.execute(
            "INSERT INTO strength_sets (activity_id, idx, start_utc, duration_s, reps, weight_kg, kind, exercise, category, ex_id, band, eff_kg)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![id, i as i64, s["startUtc"].as_i64(), s["durationS"].as_f64(), reps, w, kind, ex, s["category"].as_i64(), ex_id, band, eff],
        )
        .map_err(err)?;
    }
    tx.execute(
        "UPDATE activities SET total_sets = ?2, total_reps = ?3, weight_lifted_kg = ?4, updated_at = datetime('now') WHERE id = ?1",
        params![id, n_sets, n_reps, if lifted > 0.0 { Some(lifted) } else { None }],
    )
    .map_err(err)?;
    tx.commit().map_err(err)
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
