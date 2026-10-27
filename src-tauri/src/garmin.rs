//! Garmin through freddy (1.5): the raw payloads Garmin sends (sleep with stages, SpO₂ and
//! breathing per minute, HRV every 5 minutes, heart rate every 15 seconds, steps per 15 minutes)
//! are turned into the same rows Kouros builds from Apple Health — under provider `GARMIN` and
//! Apple's metric names — so the hypnogram, night HRV, night resting pulse, night SpO₂ and stress
//! work for a Garmin exactly as for an Apple Watch. The view `hm` takes these "native" rows when
//! a group is set to Garmin (`sources::install_view`).
//!
//! Note: Garmin's HRV is RMSSD; it is stored under `hrv_sdnn_raw` (the name the night-HRV reader
//! uses) with unit `ms_rmssd`. Norms never mix sources — a switch starts the norm again.

use crate::parse::time;
use crate::store::MetricRow;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const GARMIN: &str = "GARMIN";

/// Garmin metrics whose raw payload carries the detail (fetched with include_raw).
pub const RAW_METRICS: [&str; 3] = ["sleep_durationInSeconds", "hrv_lastNightAvg", "daily_steps"];

thread_local! {
    /// The provider code exactly as freddy writes it (rows of one source must share it).
    static CODE: std::cell::RefCell<String> = std::cell::RefCell::new(GARMIN.to_string());
}

fn row(metric: &str, date: &str, ts: &str, device: &str, source_id: &str, num: Option<f64>, txt: Option<String>, unit: &str) -> MetricRow {
    MetricRow {
        metric: metric.into(),
        date: date.into(),
        ts: ts.into(),
        provider: CODE.with(|c| c.borrow().clone()),
        device: device.into(),
        source_id: source_id.into(),
        source: Some(if device.is_empty() { "Garmin".into() } else { format!("Garmin {device}") }),
        num,
        txt,
        unit: Some(unit.into()),
    }
}

/// `{"159": 48, …}` offsets (seconds from `start`) → sorted (unix, value).
fn offsets(v: &Value, start: i64) -> Vec<(i64, f64)> {
    let mut out: Vec<(i64, f64)> = v
        .as_object()
        .map(|o| o.iter().filter_map(|(k, x)| Some((start + k.parse::<i64>().ok()?, x.as_f64()?))).collect())
        .unwrap_or_default();
    out.sort_by_key(|x| x.0);
    out
}

fn bundle(points: &[(i64, f64)]) -> String {
    Value::Array(points.iter().map(|(t, v)| json!({"t": time::iso(*t), "v": (v * 100.0).round() / 100.0})).collect()).to_string()
}

/// One night: hypnogram + 4 totals + SpO₂ readings + average breathing while asleep.
fn sleep_rows(date: &str, device: &str, r: &Value, out: &mut Vec<MetricRow>) {
    let Some(start) = r["startTimeInSeconds"].as_i64() else { return };
    let date = r["calendarDate"].as_str().unwrap_or(date);
    let sid = r["summaryId"].as_str().unwrap_or("");
    let mut segs: Vec<(&str, i64, i64)> = Vec::new();
    for (k, stage) in [("deep", "deep"), ("light", "core"), ("rem", "rem"), ("awake", "awake")] {
        for s in r["sleepLevelsMap"][k].as_array().into_iter().flatten() {
            if let (Some(a), Some(b)) = (s["startTimeInSeconds"].as_i64(), s["endTimeInSeconds"].as_i64()) {
                if b > a {
                    segs.push((stage, a, b));
                }
            }
        }
    }
    segs.sort_by_key(|s| s.1);
    if !segs.is_empty() {
        let txt = Value::Array(segs.iter().map(|(s, a, b)| json!({"stage": s, "start": time::iso(*a), "end": time::iso(*b)})).collect()).to_string();
        out.push(row("sleep_analysis_raw", date, date, device, sid, None, Some(txt), ""));
    }
    let total = |stage: &str, key: &str| -> f64 {
        r[key].as_f64().unwrap_or_else(|| segs.iter().filter(|s| s.0 == stage).map(|s| (s.2 - s.1) as f64).sum())
    };
    for (m, stage, key) in [
        ("sleep_analysis_deep_seconds", "deep", "deepSleepDurationInSeconds"),
        ("sleep_analysis_core_seconds", "core", "lightSleepDurationInSeconds"),
        ("sleep_analysis_rem_seconds", "rem", "remSleepInSeconds"),
        ("sleep_analysis_awake_seconds", "awake", "awakeDurationInSeconds"),
    ] {
        out.push(row(m, date, date, device, sid, Some(total(stage, key)), None, "s"));
    }
    // SpO₂ every minute → one reading per 5 minutes is plenty for the night median.
    for (i, (t, v)) in offsets(&r["timeOffsetSleepSpo2"], start).into_iter().enumerate() {
        if i % 5 == 0 && (70.0..=100.0).contains(&v) {
            out.push(row("blood_oxygen", date, &time::iso(t), device, "", Some(v), None, "percent"));
        }
    }
    let resp: Vec<f64> = offsets(&r["timeOffsetSleepRespiration"], start).into_iter().map(|x| x.1).filter(|v| (4.0..=40.0).contains(v)).collect();
    if resp.len() >= 10 {
        let avg = resp.iter().sum::<f64>() / resp.len() as f64;
        out.push(row("respiratory_rate_merged_avg", date, date, device, "", Some((avg * 10.0).round() / 10.0), None, "count/min"));
    }
}

/// HRV of a night: samples every 5 minutes + Garmin's night average.
fn hrv_rows(date: &str, device: &str, r: &Value, out: &mut Vec<MetricRow>) {
    let Some(start) = r["startTimeInSeconds"].as_i64() else { return };
    let date = r["calendarDate"].as_str().unwrap_or(date);
    let pts: Vec<(i64, f64)> = offsets(&r["hrvValues"], start).into_iter().filter(|x| (5.0..=300.0).contains(&x.1)).collect();
    if !pts.is_empty() {
        out.push(row("hrv_sdnn_raw", date, date, device, "", None, Some(bundle(&pts)), "ms_rmssd"));
    }
    if let Some(a) = r["lastNightAvg"].as_f64() {
        out.push(row("hrv_rmssd_merged_avg", date, date, device, "", Some(a), None, "ms_rmssd"));
    }
}

/// Heart rate of a day (every 15 s, the same value repeated within a minute) → one point a minute.
fn pulse_rows(date: &str, device: &str, r: &Value, out: &mut Vec<MetricRow>) {
    let Some(start) = r["startTimeInSeconds"].as_i64() else { return };
    let date = r["calendarDate"].as_str().unwrap_or(date);
    let mut per: BTreeMap<i64, f64> = BTreeMap::new();
    for (t, v) in offsets(&r["timeOffsetHeartRateSamples"], start) {
        if (25.0..=230.0).contains(&v) {
            per.entry(t - t.rem_euclid(60)).or_insert(v);
        }
    }
    if !per.is_empty() {
        let pts: Vec<(i64, f64)> = per.into_iter().collect();
        out.push(row("heart_rate_raw", date, date, device, "", None, Some(bundle(&pts)), "bpm"));
    }
}

/// Builds the Apple-shaped rows from the Garmin rows stored since `from` (YYYY-MM-DD).
pub fn derive(conn: &mut Connection, from: &str) -> Result<usize, String> {
    let Ok(code) = conn.query_row("SELECT provider FROM health_metrics WHERE upper(provider) = ?1 LIMIT 1", params![GARMIN], |r| r.get::<_, String>(0)) else {
        return Ok(0);
    };
    CODE.with(|c| *c.borrow_mut() = code.clone());
    let list = RAW_METRICS.iter().map(|m| format!("'{m}'")).collect::<Vec<_>>().join(",");
    let raws: Vec<(String, String, String, String)> = {
        let mut st = conn
            .prepare(&format!("SELECT metric, date, COALESCE(device, ''), txt FROM health_metrics WHERE provider = ?1 AND metric IN ({list}) AND date >= ?2 AND txt IS NOT NULL"))
            .map_err(|e| e.to_string())?;
        let v = st.query_map(params![code, from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|e| e.to_string())?.flatten().collect();
        v
    };
    let mut out: Vec<MetricRow> = Vec::new();
    for (metric, date, device, txt) in raws {
        let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
        match metric.as_str() {
            "sleep_durationInSeconds" => sleep_rows(&date, &device, &v, &mut out),
            "hrv_lastNightAvg" => hrv_rows(&date, &device, &v, &mut out),
            "daily_steps" => pulse_rows(&date, &device, &v, &mut out),
            _ => {}
        }
    }
    // Steps per 15-minute epoch (several rows per epoch, one per activity type) → per minute.
    let epochs: Vec<(String, String, String, f64)> = {
        let mut st = conn
            .prepare("SELECT date, COALESCE(ts, ''), COALESCE(device, ''), sum(num) FROM health_metrics WHERE provider = ?1 AND metric = 'epoch_steps' AND date >= ?2 AND num IS NOT NULL GROUP BY 1, 2, 3")
            .map_err(|e| e.to_string())?;
        let v = st.query_map(params![code, from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|e| e.to_string())?.flatten().collect();
        v
    };
    let mut days: BTreeMap<(String, String), Vec<(i64, f64)>> = BTreeMap::new();
    for (date, ts, device, steps) in epochs {
        let Some(t) = crate::health_svc::iso(&ts) else { continue };
        let e = days.entry((date, device)).or_default();
        for k in 0..15 {
            e.push((t + k * 60, steps / 15.0));
        }
    }
    for ((date, device), mut pts) in days {
        pts.sort_by_key(|x| x.0);
        out.push(row("steps_raw", &date, &date, &device, "", None, Some(bundle(&pts)), "count"));
    }
    let n = out.len();
    if n > 0 {
        crate::store::upsert_metrics(conn, &out)?;
    }
    Ok(n)
}
