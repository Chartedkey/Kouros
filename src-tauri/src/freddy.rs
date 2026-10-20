//! freddy personal API (Apple Health, Garmin, Suunto, Fitbit… hub): https://freddy.coach/api/v1/
//! Read-only, Bearer key `fk_live_…`, cursor paging, 120 requests/min.

use crate::secrets;
use crate::store::MetricRow;
use serde_json::Value;
use std::time::Duration;

const BASE: &str = "https://freddy.coach/api/v1";
const PAGE: usize = 500;
const VPN: &str = " Похоже, freddy недоступен из этой сети — включи VPN для Kouros и нажми «Обновить сейчас».";

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(180))
        .user_agent("Kouros/0.3")
        .build()
        .map_err(|e| e.to_string())
}

fn key() -> Result<String, String> {
    secrets::get("freddy_api_key")?.ok_or_else(|| "Сначала сохрани API-ключ freddy".to_string())
}

/// Workouts come only from Strava; raw sample bundles are not needed yet.
pub fn keep_metric(name: &str) -> bool {
    // The sleep hypnogram (stage by stage) is the one raw bundle we keep.
    if matches!(name, "sleep_analysis_raw" | "hrv_sdnn_raw" | "heart_rate_raw" | "steps_raw" | "workout_raw") {
        return true;
    }
    // Workout summaries are kept since 0.17 (workouts from freddy); their raw bundles are not.
    !(name.starts_with("running_")
        || name.ends_with("_raw")
        || name.starts_with("estimated_workout_"))
}

async fn get(url: &str, key: &str) -> Result<Value, String> {
    let mut last = String::from("freddy не ответил");
    for attempt in 0..5u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(3 * attempt)).await;
        }
        let resp = match http()?
            .get(url)
            .bearer_auth(key)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last = format!("freddy недоступен: {e}{}.{VPN}", if e.is_timeout() { " (истекло время ожидания)" } else { "" });
                continue;
            }
        };
        let code = resp.status().as_u16();
        if code == 429 {
            let wait = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30)
                .min(120);
            last = "freddy просит подождать — попробую позже.".into();
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        if code == 401 {
            return Err("freddy не принял ключ — проверь API-ключ в настройках.".into());
        }
        let ctype = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        // Read the whole body first: a body cut off mid-way is retried, not misreported.
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                last = format!(
                    "ответ freddy оборвался{}: {e}.{VPN}",
                    if e.is_timeout() { " (истекло время ожидания)" } else { "" }
                );
                continue;
            }
        };
        if !(200..300).contains(&code) {
            let head: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
            last = format!("freddy ответил {code}: {head}{}", if code == 403 || head.trim_start().starts_with('<') { VPN } else { "" });
            if code >= 500 {
                continue;
            }
            return Err(last);
        }
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) => return Ok(v),
            Err(e) => {
                let head: String = String::from_utf8_lossy(&bytes).chars().take(160).collect();
                last = format!(
                    "freddy прислал непонятный ответ ({e}; {} байт, {ctype}): {head}{}",
                    bytes.len(),
                    if ctype.contains("html") || head.trim_start().starts_with('<') { VPN } else { "" }
                );
            }
        }
    }
    Err(last)
}

/// Checks the key; returns the plan's history window if freddy reports it.
pub async fn check() -> Result<String, String> {
    let k = key()?;
    let v = get(&format!("{BASE}/profile"), &k).await?;
    let plan = v["plan"].as_str().or(v["data"]["plan"].as_str()).unwrap_or("").to_string();
    Ok(plan)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn row(v: &Value) -> Option<MetricRow> {
    let metric = v["metric"].as_str()?.to_string();
    if !keep_metric(&metric) || v["stream"].as_bool() == Some(true) {
        return None;
    }
    let date = v["date"].as_str()?.chars().take(10).collect::<String>();
    Some(MetricRow {
        metric,
        date,
        ts: s(&v["timestamp"]),
        provider: s(&v["provider"]),
        device: s(&v["device"]),
        source_id: s(&v["sourceId"]),
        source: v["deviceLabel"].as_str().or(v["source"].as_str()).map(|x| x.to_string()),
        num: v["numValue"].as_f64(),
        txt: v["textValue"].as_str().map(|x| x.to_string()).or_else(|| raw_payload(v)),
        unit: v["unit"].as_str().map(|x| x.to_string()),
    })
}

/// The raw JSON payload (hypnogram) under whichever name the API uses for it.
fn raw_payload(v: &Value) -> Option<String> {
    for k in ["raw", "rawData", "raw_data", "rawValue", "payload", "rawJson"] {
        match &v[k] {
            Value::Null => continue,
            Value::String(s) if !s.is_empty() => return Some(s.clone()),
            Value::Array(_) | Value::Object(_) => return Some(v[k].to_string()),
            _ => continue,
        }
    }
    None
}

/// Raw bundles (sleep stages, HRV samples) for one metric since `from`. Best effort:
/// if freddy does not support the filter, nothing breaks.
pub async fn raw_rows(metric: &str, from: &str) -> Result<Vec<MetricRow>, String> {
    let k = key()?;
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..30 {
        let mut url = format!("{BASE}/metrics?metric={metric}&metrics={metric}&include_raw=true&from={from}&limit=60");
        if let Some(c) = &cursor {
            let enc: String = c
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                    _ => format!("%{b:02X}"),
                })
                .collect();
            url.push_str(&format!("&cursor={enc}"));
        }
        let v = get(&url, &k).await?;
        let data = v["data"].as_array().cloned().unwrap_or_default();
        out.extend(data.iter().filter(|d| d["metric"].as_str() == Some(metric)).filter_map(row).filter(|r| r.txt.is_some()));
        match v["next_cursor"].as_str() {
            Some(n) if !data.is_empty() => cursor = Some(n.to_string()),
            _ => break,
        }
    }
    Ok(out)
}

pub struct Page {
    pub rows: Vec<MetricRow>,
    pub next: Option<String>,
    pub max_date: Option<String>,
    pub seen: usize,
}

/// One page of metrics from `from` (YYYY-MM-DD, inclusive) on.
pub async fn page(from: Option<&str>, cursor: Option<&str>, provider: Option<&str>) -> Result<Page, String> {
    page_m(from, cursor, provider, None).await
}

/// One page, optionally only some metrics (comma-separated).
pub async fn page_m(from: Option<&str>, cursor: Option<&str>, provider: Option<&str>, metrics: Option<&str>) -> Result<Page, String> {
    let k = key()?;
    let mut url = format!("{BASE}/metrics?limit={PAGE}");
    if let Some(m) = metrics {
        url.push_str(&format!("&metric={m}"));
    }
    if let Some(p) = provider {
        url.push_str(&format!("&provider={p}"));
    }
    if let Some(f) = from {
        url.push_str(&format!("&from={f}"));
    }
    if let Some(c) = cursor {
        let enc: String = c
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect();
        url.push_str(&format!("&cursor={enc}"));
    }
    let v = get(&url, &k).await?;
    let data = v["data"].as_array().cloned().unwrap_or_default();
    let max_date = data.iter().filter_map(|d| d["date"].as_str()).map(|d| d.chars().take(10).collect::<String>()).max();
    let rows: Vec<MetricRow> = data.iter().filter_map(row).collect();
    Ok(Page { seen: data.len(), rows, next: v["next_cursor"].as_str().map(|s| s.to_string()), max_date })
}
