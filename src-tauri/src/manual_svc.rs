//! Values the athlete types in himself (1.4, owner): morning HRV from another app or a chest
//! strap, and — if he wants — breathing rate and SpO₂. Stored in `health_metrics` as provider
//! `MANUAL` (`hrv_manual`, `respiratory_manual`, `spo2_manual`); `sources` maps them onto the
//! HRV / vitals groups like any other source, so the norm, readiness and charts work as usual.
//! RMSSD and SDNN are different numbers: the kind is kept in settings `manual_hrv` {kind}, and
//! changing it starts the HRV norm again (a device reset from that day).

use crate::{db, sources};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const M_HRV: &str = "hrv_manual";
const M_RESP: &str = "respiratory_manual";
const M_SPO2: &str = "spo2_manual";

fn cfg(conn: &Connection) -> Value {
    db::get_setting(conn, "manual_hrv").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({"kind": "rmssd"}))
}

fn values(conn: &Connection, metric: &str, from: &str) -> Vec<(String, f64)> {
    conn.prepare("SELECT date, num FROM health_metrics WHERE provider = ?1 AND metric = ?2 AND date >= ?3 ORDER BY date")
        .and_then(|mut st| st.query_map(params![sources::MANUAL, metric, from], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

fn median_of(v: &[(String, f64)], before: &str) -> Option<f64> {
    let x: Vec<f64> = v.iter().filter(|r| r.0.as_str() < before).map(|r| r.1).collect();
    if x.len() < 3 { None } else { crate::engine::median(&x) }
}

/// For the morning card and Settings: which values are typed by hand, today's entry, the usual
/// level (median of 30 days) for the «точно?» check, and the last two weeks.
pub fn state(conn: &Connection) -> Value {
    let p = crate::engine_svc::profile(conn);
    let today = crate::engine_svc::today(&p);
    let from = crate::engine::add_days(&today, -30);
    let hrv_on = sources::current(conn, "hrv") == sources::MANUAL;
    let vit_on = sources::current(conn, "vitals") == sources::MANUAL;
    let (h, r, s) = (values(conn, M_HRV, &from), values(conn, M_RESP, &from), values(conn, M_SPO2, &from));
    let on = |v: &[(String, f64)], d: &str| v.iter().find(|x| x.0 == d).map(|x| x.1);
    let mut days: Vec<String> = h.iter().chain(r.iter()).chain(s.iter()).map(|x| x.0.clone()).collect();
    days.sort();
    days.dedup();
    json!({
        "hrvOn": hrv_on, "vitalsOn": vit_on, "kind": cfg(conn)["kind"], "today": today,
        "values": {"hrv": on(&h, &today), "resp": on(&r, &today), "spo2": on(&s, &today)},
        "usual": {"hrv": median_of(&h, &today), "resp": median_of(&r, &today), "spo2": median_of(&s, &today)},
        "history": days.iter().rev().take(14).map(|d| json!({"date": d, "hrv": on(&h, d), "resp": on(&r, d), "spo2": on(&s, d)})).collect::<Vec<_>>(),
    })
}

fn put(conn: &Connection, metric: &str, date: &str, v: Option<f64>, unit: &str, txt: Option<&str>) -> Result<(), String> {
    conn.execute("DELETE FROM health_metrics WHERE provider = ?1 AND metric = ?2 AND date = ?3", params![sources::MANUAL, metric, date]).map_err(|e| e.to_string())?;
    if let Some(v) = v {
        // One row a day, at local noon of that day (an edit replaces it).
        let ts = crate::engine::day_num(date).unwrap_or(0) * 86400 + 12 * 3600;
        conn.execute(
            "INSERT INTO health_metrics (metric, date, ts, provider, device, source_id, source, num, txt, unit) VALUES (?1, ?2, ?3, ?4, '', '', 'Вручную', ?5, ?6, ?7)",
            params![metric, date, ts, sources::MANUAL, v, txt, unit],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Saves the values of a day (None = remove). Plausible ranges only: HRV 5–300 ms, breathing
/// 5–40 /min, SpO₂ 70–100 %.
pub fn save(conn: &Connection, date: &str, v: &Value) -> Result<(), String> {
    if crate::engine::day_num(date).is_none() {
        return Err("Неверная дата".into());
    }
    let mut c = cfg(conn);
    if let Some(k) = v["kind"].as_str().filter(|k| matches!(*k, "rmssd" | "sdnn")) {
        if c["kind"].as_str() != Some(k) {
            // Another kind of HRV: the old norm doesn't fit — start it again from this day.
            let had = conn.query_row("SELECT count(*) FROM health_metrics WHERE provider = ?1 AND metric = ?2", params![sources::MANUAL, M_HRV], |r| r.get::<_, i64>(0)).unwrap_or(0);
            if had > 0 {
                sources::device_decide(conn, &[json!({"group": "hrv", "to": format!("manual-{k}"), "since": date})], true)?;
            }
            c["kind"] = json!(k);
            db::set_setting(conn, "manual_hrv", &c.to_string())?;
        }
    }
    let rng = |x: &Value, lo: f64, hi: f64, what: &str| -> Result<Option<f64>, String> {
        match x.as_f64() {
            None => Ok(None),
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            Some(_) => Err(format!("{what}: похоже на опечатку (обычно {lo}–{hi}).")),
        }
    };
    let kind = c["kind"].as_str().unwrap_or("rmssd").to_string();
    if v.get("hrv").is_some() {
        put(conn, M_HRV, date, rng(&v["hrv"], 5.0, 300.0, "HRV")?, "ms", Some(kind.as_str()))?;
    }
    if v.get("resp").is_some() {
        put(conn, M_RESP, date, rng(&v["resp"], 5.0, 40.0, "Дыхание")?, "count/min", None)?;
    }
    if v.get("spo2").is_some() {
        put(conn, M_SPO2, date, rng(&v["spo2"], 70.0, 100.0, "SpO₂")?, "%", None)?;
    }
    // Today's frozen morning score was made without this value.
    let _ = conn.execute("DELETE FROM readiness_log WHERE date = ?1", params![date]);
    sources::install_view(conn)
}
