//! Strava API: OAuth sign-in through a one-shot local web page, token refresh,
//! incremental sync of new workouts and gradual download of sample streams.

use crate::parse::{self, time, Parsed, Sample, Streams};
use crate::{secrets, store};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const API: &str = "https://www.strava.com/api/v3";
const TOKEN_URL: &str = "https://www.strava.com/oauth/token";
const AUTH_URL: &str = "https://www.strava.com/oauth/authorize";
const SCOPES: &str = "read,activity:read_all,profile:read_all";
/// Streams fetched per sync run, to stay well inside Strava's daily limit.
const STREAMS_PER_RUN: usize = 40;

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Kouros/0.3.2")
        .build()
        .map_err(|e| e.to_string())
}

fn client_creds() -> Result<(String, String), String> {
    let id = secrets::get("strava_client_id")?.ok_or("Сначала сохрани Client ID Strava")?;
    let secret = secrets::get("strava_client_secret")?.ok_or("Сначала сохрани Client Secret Strava")?;
    Ok((id, secret))
}

fn save_tokens(v: &Value) -> Result<(), String> {
    let access = v["access_token"].as_str().ok_or("Strava не вернула токен")?;
    let refresh = v["refresh_token"].as_str().ok_or("Strava не вернула refresh-токен")?;
    let exp = v["expires_at"].as_i64().unwrap_or(0);
    secrets::set("strava_access_token", access)?;
    secrets::set("strava_refresh_token", refresh)?;
    secrets::set("strava_expires_at", &exp.to_string())?;
    Ok(())
}

pub fn is_connected() -> bool {
    secrets::has("strava_refresh_token")
}

pub fn disconnect() -> Result<(), String> {
    for k in ["strava_access_token", "strava_refresh_token", "strava_expires_at"] {
        secrets::delete(k)?;
    }
    Ok(())
}

/// Turns a Strava error response into a plain explanation.
fn explain(code: u16, body: &str, context: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let errs = v["errors"].as_array().cloned().unwrap_or_default();
    let has = |field: &str, c: &str| {
        errs.iter().any(|e| {
            e["field"].as_str().map(|f| f.eq_ignore_ascii_case(field)).unwrap_or(false)
                && e["code"].as_str().map(|x| x.eq_ignore_ascii_case(c)).unwrap_or(false)
        })
    };
    let detail: String = errs
        .iter()
        .map(|e| {
            format!(
                "{} {} {}",
                e["resource"].as_str().unwrap_or(""),
                e["field"].as_str().unwrap_or(""),
                e["code"].as_str().unwrap_or("")
            )
            .trim()
            .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let raw = if detail.is_empty() { body.chars().take(160).collect::<String>() } else { detail };
    if has("status", "inactive") || (code == 403 && raw.to_lowercase().contains("inactive")) {
        return "Strava считает приложение Kouros неактивным. Открой strava.com/settings/api и активируй доступ к API \
                (для своих приложений Strava требует действующую подписку), затем нажми «Отключить» и «Подключить Strava»."
            .into();
    }
    if has("client_id", "invalid") || has("client_secret", "invalid") {
        return "Strava не узнаёт Client ID или Client Secret — сверь их со страницей strava.com/settings/api.".into();
    }
    if has("refresh_token", "invalid") || code == 401 {
        return "Вход в Strava устарел — нажми «Отключить» и «Подключить Strava» заново.".into();
    }
    let looks_blocked = body.trim().is_empty() || body.trim_start().starts_with('<') || errs.is_empty();
    if code == 403 && looks_blocked {
        return "Strava не пускает запрос из этой сети (403). Похоже, Strava недоступна без VPN — включи VPN для Kouros \
                и нажми «Обновить сейчас». Данные не пропадут: Kouros продолжит с того же места."
            .into();
    }
    if code == 403 {
        return format!("Strava запретила доступ (403): {raw}. Проверь статус приложения на strava.com/settings/api.");
    }
    format!("{context}: Strava ответила {code}. {raw}")
}

async fn token() -> Result<String, String> {
    let exp: i64 = secrets::get("strava_expires_at")?.and_then(|s| s.parse().ok()).unwrap_or(0);
    if let Some(t) = secrets::get("strava_access_token")? {
        if exp - 300 > store::now() {
            return Ok(t);
        }
    }
    let refresh = secrets::get("strava_refresh_token")?.ok_or("Strava не подключена")?;
    let (id, secret) = client_creds()?;
    let resp = http()?
        .post(TOKEN_URL)
        .form(&[
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Strava недоступна: {e}. Если без VPN Strava не открывается — включи VPN для Kouros."))?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(explain(code, &body, "Обновление входа"));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    save_tokens(&v)?;
    Ok(v["access_token"].as_str().unwrap_or_default().to_string())
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn query_param(q: &str, key: &str) -> Option<String> {
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key {
            Some(v.replace('+', " "))
        } else {
            None
        }
    })
}

const DONE_PAGE: &str = "<!doctype html><html lang=ru><meta charset=utf-8><title>Kouros</title>\
<body style=\"background:#0B0D10;color:#F1F3F5;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><div style=\"font-size:22px;font-weight:700\">Strava подключена к Kouros</div>\
<div style=\"color:#9AA3AE;margin-top:8px\">Можно закрыть эту вкладку и вернуться в приложение.</div></div></body></html>";

const FAIL_PAGE: &str = "<!doctype html><html lang=ru><meta charset=utf-8><title>Kouros</title>\
<body style=\"background:#0B0D10;color:#F1F3F5;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><div style=\"font-size:22px;font-weight:700\">Доступ не выдан</div>\
<div style=\"color:#9AA3AE;margin-top:8px\">Вернись в Kouros и попробуй ещё раз.</div></div></body></html>";

/// Waits (blocking) for the browser to come back with `?code=…`.
fn wait_for_code(listener: TcpListener, state: &str) -> Result<(String, String), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if Instant::now() > deadline {
            return Err("Время ожидания вышло — окно Strava не вернуло ответ за 5 минут.".into());
        }
        match listener.accept() {
            Ok((mut sock, _)) => {
                let _ = sock.set_nonblocking(false);
                let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("");
                let path = line.split_whitespace().nth(1).unwrap_or("");
                if !path.starts_with("/strava") {
                    let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    continue;
                }
                let q = path.split_once('?').map(|(_, q)| q).unwrap_or("");
                let ok = query_param(q, "state").as_deref() == Some(state) && query_param(q, "code").is_some();
                let body = if ok { DONE_PAGE } else { FAIL_PAGE };
                let _ = sock.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                );
                if !ok {
                    return Err(match query_param(q, "error") {
                        Some(e) if e == "access_denied" => "Доступ к Strava не выдан.".into(),
                        Some(e) => format!("Strava вернула ошибку: {e}"),
                        None => "Strava вернула неожиданный ответ.".into(),
                    });
                }
                let scope = query_param(q, "scope").unwrap_or_default();
                return Ok((query_param(q, "code").unwrap_or_default(), scope));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Builds the sign-in URL and a listener for the redirect.
pub fn begin_auth() -> Result<(String, TcpListener, String), String> {
    let (id, _) = client_creds()?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Не удалось открыть порт: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_hex(16);
    let redirect = format!("http://127.0.0.1:{port}/strava");
    let url = format!(
        "{AUTH_URL}?client_id={id}&response_type=code&approval_prompt=auto&scope={SCOPES}&state={state}&redirect_uri={}",
        redirect.replace(':', "%3A").replace('/', "%2F")
    );
    Ok((url, listener, state))
}

/// Finishes sign-in; returns the athlete's display name.
pub async fn finish_auth(listener: TcpListener, state: String) -> Result<String, String> {
    let (code, scope) = tauri::async_runtime::spawn_blocking(move || wait_for_code(listener, &state))
        .await
        .map_err(|e| e.to_string())??;
    if !scope.contains("activity:read_all") && !scope.contains("activity:read") {
        return Err("Нужно разрешить доступ к тренировкам — отметь галочку «просмотр данных о тренировках».".into());
    }
    let (id, secret) = client_creds()?;
    let resp = http()?
        .post(TOKEN_URL)
        .form(&[
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| format!("Strava недоступна: {e}. Если без VPN Strava не открывается — включи VPN для Kouros."))?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(explain(code, &body, "Вход в Strava"));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    save_tokens(&v)?;
    let a = &v["athlete"];
    let name = format!(
        "{} {}",
        a["firstname"].as_str().unwrap_or(""),
        a["lastname"].as_str().unwrap_or("")
    )
    .trim()
    .to_string();
    Ok(name)
}

fn f(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

pub fn activity_from_api(v: &Value) -> Option<(i64, Parsed)> {
    let id = v["id"].as_i64()?;
    let start = v["start_date"].as_str().and_then(time::parse_iso)?;
    let label = v["sport_type"].as_str().or(v["type"].as_str()).map(|s| s.to_string());
    let p = Parsed {
        sport: parse::sport_from_strava(label.as_deref().unwrap_or("")).to_string(),
        sport_label: label,
        name: v["name"].as_str().map(|s| s.to_string()),
        description: v["description"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string()),
        start_utc: start,
        elapsed_s: f(&v["elapsed_time"]),
        moving_s: f(&v["moving_time"]),
        distance_m: f(&v["distance"]).filter(|d| *d > 0.0),
        elev_gain_m: f(&v["total_elevation_gain"]),
        avg_hr: f(&v["average_heartrate"]),
        max_hr: f(&v["max_heartrate"]),
        avg_cadence: f(&v["average_cadence"]),
        avg_power: f(&v["average_watts"]),
        calories: f(&v["calories"]),
        relative_effort: f(&v["suffer_score"]),
        gear: v["gear_id"].as_str().map(|s| s.to_string()),
        device: v["device_name"].as_str().map(|s| s.to_string()),
        ..Default::default()
    };
    Some((id, p))
}

fn streams_from_api(v: &Value) -> Streams {
    let arr = |k: &str| v[k]["data"].as_array().cloned().unwrap_or_default();
    let t = arr("time");
    let (hr, dist, spd, alt, cad, pwr, ll) =
        (arr("heartrate"), arr("distance"), arr("velocity_smooth"), arr("altitude"), arr("cadence"), arr("watts"), arr("latlng"));
    let at = |a: &Vec<Value>, i: usize| a.get(i).and_then(|x| x.as_f64());
    let mut s = Streams::default();
    for (i, tv) in t.iter().enumerate() {
        let Some(tt) = tv.as_i64() else { continue };
        let (lat, lng) = match ll.get(i).and_then(|x| x.as_array()) {
            Some(p) if p.len() == 2 => (p[0].as_f64(), p[1].as_f64()),
            _ => (None, None),
        };
        s.push(
            tt,
            Sample {
                hr: at(&hr, i).filter(|h| *h > 0.0),
                dist: at(&dist, i),
                speed: at(&spd, i),
                alt: at(&alt, i),
                cad: at(&cad, i),
                pwr: at(&pwr, i),
                lat,
                lng,
            },
        );
    }
    s
}

pub enum Fetch<T> {
    Ok(T),
    RateLimited,
}

async fn get_json(tok: &str, url: &str) -> Result<Fetch<Value>, String> {
    let resp = http()?
        .get(url)
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|e| format!("Strava недоступна: {e}. Если без VPN Strava не открывается — включи VPN для Kouros."))?;
    if resp.status().as_u16() == 429 {
        return Ok(Fetch::RateLimited);
    }
    if resp.status().as_u16() == 404 {
        return Ok(Fetch::Ok(Value::Null));
    }
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(explain(code, &body, "Загрузка тренировок"));
    }
    resp.json().await.map(Fetch::Ok).map_err(|e| e.to_string())
}

/// Lists activities that started after `after` (Unix seconds), all pages.
pub async fn list_after(after: i64) -> Result<(Vec<(i64, Parsed)>, bool), String> {
    let tok = token().await?;
    let mut out = Vec::new();
    for page in 1..=50 {
        let url = format!("{API}/athlete/activities?after={after}&per_page=200&page={page}");
        match get_json(&tok, &url).await? {
            Fetch::RateLimited => return Ok((out, true)),
            Fetch::Ok(v) => {
                let items = v.as_array().cloned().unwrap_or_default();
                if items.is_empty() {
                    break;
                }
                out.extend(items.iter().filter_map(activity_from_api));
                if items.len() < 200 {
                    break;
                }
            }
        }
    }
    Ok((out, false))
}

pub async fn streams(strava_id: i64) -> Result<Fetch<Streams>, String> {
    let tok = token().await?;
    let url = format!(
        "{API}/activities/{strava_id}/streams?keys=time,heartrate,distance,velocity_smooth,altitude,cadence,watts,latlng&key_by_type=true"
    );
    Ok(match get_json(&tok, &url).await? {
        Fetch::RateLimited => Fetch::RateLimited,
        Fetch::Ok(v) => Fetch::Ok(streams_from_api(&v)),
    })
}

pub fn streams_per_run() -> usize {
    STREAMS_PER_RUN
}
