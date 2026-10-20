//! Psychology tab: questionnaires and how the head copes with the load.
//! - Hooper index (morning): sleep, stress, fatigue, muscle soreness, 1–7 each (4–28, lower is better).
//! - TQR (before training): total quality of recovery, 6–20.
//! - BRUMS (weekly): 24 mood words, 0–4 each, six scales of 0–16.
//! - Stress and recovery (every two weeks): Kouros' own 8 questions, 0–6 each.

use crate::engine::add_days;
use crate::engine_svc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// BRUMS scales in the order of the 24 answers (4 words each).
pub const BRUMS_SCALES: [&str; 6] = ["tension", "depression", "anger", "vigour", "fatigue", "confusion"];

fn nums(a: &Value) -> Vec<f64> {
    a.as_array().map(|v| v.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default()
}

/// Validates the answers and returns (score, extra).
fn score(kind: &str, a: &Value) -> Result<(f64, Value), String> {
    let v = nums(a);
    match kind {
        "hooper" => {
            if v.len() != 4 || v.iter().any(|x| !(1.0..=7.0).contains(x)) {
                return Err("Нужно 4 ответа от 1 до 7".into());
            }
            let sum: f64 = v.iter().sum();
            // 0–100 where 100 = feeling great (4 points) and 0 = awful (28).
            let feel = ((28.0 - sum) / 24.0 * 100.0).round();
            Ok((sum, json!({"feel": feel})))
        }
        "tqr" => {
            if v.len() != 1 || !(6.0..=20.0).contains(&v[0]) {
                return Err("Оценка от 6 до 20".into());
            }
            Ok((v[0], Value::Null))
        }
        "brums" => {
            if v.len() != 24 || v.iter().any(|x| !(0.0..=4.0).contains(x)) {
                return Err("Нужно 24 ответа от 0 до 4".into());
            }
            let scales: serde_json::Map<String, Value> =
                BRUMS_SCALES.iter().enumerate().map(|(i, k)| (k.to_string(), json!(v[i * 4..i * 4 + 4].iter().sum::<f64>()))).collect();
            let g = |k: &str| scales.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
            // Total mood disturbance: negative scales minus vigour (lower is better).
            let tmd = g("tension") + g("depression") + g("anger") + g("fatigue") + g("confusion") - g("vigour");
            Ok((tmd, Value::Object(scales)))
        }
        "stressrec" => {
            if v.len() != 8 || v.iter().any(|x| !(0.0..=6.0).contains(x)) {
                return Err("Нужно 8 ответов от 0 до 6".into());
            }
            let stress = v[..4].iter().sum::<f64>() / 4.0;
            let rec = v[4..].iter().sum::<f64>() / 4.0;
            Ok((((rec - stress) * 10.0).round() / 10.0, json!({"stress": (stress * 10.0).round() / 10.0, "recovery": (rec * 10.0).round() / 10.0})))
        }
        _ => Err("Неизвестный опрос".into()),
    }
}

pub fn save(conn: &Connection, kind: &str, answers: &Value, date: Option<&str>) -> Result<Value, String> {
    let (sc, extra) = score(kind, answers)?;
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let date = date.unwrap_or(&today);
    // One morning check-in per day: a new answer replaces the old one.
    if kind == "hooper" {
        conn.execute("DELETE FROM surveys WHERE kind = 'hooper' AND date = ?1", params![date]).map_err(err)?;
    }
    conn.execute(
        "INSERT INTO surveys (kind, date, ts, answers, score, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![kind, date, crate::store::now(), answers.to_string(), sc, extra.to_string()],
    )
    .map_err(err)?;
    Ok(json!({"score": sc, "extra": extra}))
}

fn rows(conn: &Connection, kind: &str, from: &str) -> Result<Vec<Value>, String> {
    let mut st = conn
        .prepare("SELECT date, ts, answers, score, extra FROM surveys WHERE kind = ?1 AND date >= ?2 ORDER BY ts")
        .map_err(err)?;
    let out = st
        .query_map(params![kind, from], |r| {
            let a: String = r.get(2)?;
            let e: Option<String> = r.get(4)?;
            Ok(json!({"date": r.get::<_, String>(0)?, "ts": r.get::<_, i64>(1)?,
                      "answers": serde_json::from_str::<Value>(&a).unwrap_or(Value::Null), "score": r.get::<_, Option<f64>>(3)?,
                      "extra": e.and_then(|x| serde_json::from_str::<Value>(&x).ok()).unwrap_or(Value::Null)}))
        })
        .map_err(err)?
        .flatten()
        .collect();
    Ok(out)
}

pub fn summary(conn: &Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let tz = p.tz_offset_s;
    let hooper = rows(conn, "hooper", &add_days(&today, -60))?;
    let tqr = rows(conn, "tqr", &add_days(&today, -30))?;
    let brums = rows(conn, "brums", &add_days(&today, -120))?;
    let sr = rows(conn, "stressrec", &add_days(&today, -180))?;
    let last_date = |v: &[Value]| v.last().and_then(|x| x["date"].as_str()).map(|s| s.to_string());
    let days_since = |d: Option<String>| d.and_then(|d| Some(crate::engine::day_num(&today)? - crate::engine::day_num(&d)?));

    // Workouts of the last 3 days that still have no effort rating.
    let since = crate::store::now() - 3 * 86400;
    let mut st = conn
        .prepare("SELECT id, sport, name, start_utc FROM va WHERE start_utc >= ?1 AND rpe IS NULL ORDER BY start_utc DESC LIMIT 6")
        .map_err(err)?;
    let unrated: Vec<Value> = st
        .query_map(params![since], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?, "startUtc": r.get::<_, i64>(3)?}))
        })
        .map_err(err)?
        .flatten()
        .collect();
    // Training today (for the TQR reminder): any workout started today or none yet.
    let midnight = crate::engine::day_num(&today).map(|d| d * 86400 - tz).unwrap_or(0);
    let trained_today: i64 = conn
        .query_row("SELECT count(*) FROM va WHERE start_utc >= ?1", params![midnight], |r| r.get(0))
        .unwrap_or(0);

    Ok(json!({
        "today": today,
        "hooper": hooper,
        "tqr": tqr,
        "brums": brums,
        "stressrec": sr,
        "unrated": unrated,
        "trainedToday": trained_today,
        "due": {
            "hooper": last_date(&hooper).as_deref() != Some(today.as_str()),
            "tqr": last_date(&tqr).as_deref() != Some(today.as_str()),
            "brumsDays": days_since(last_date(&brums)),
            "stressrecDays": days_since(last_date(&sr)),
        },
    }))
}

pub fn delete(conn: &Connection, kind: &str, date: &str) -> Result<(), String> {
    conn.execute("DELETE FROM surveys WHERE kind = ?1 AND date = ?2", params![kind, date]).map(|_| ()).map_err(err)
}
