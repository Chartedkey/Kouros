use crate::{ai, ai_context, guard, planning, secrets, with_db, with_db_mut, AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
#[tauri::command]
pub fn ai_state(st: State<'_, AppState>) -> Result<Value, String> {
    with_db(&st, |c| {
        let today = crate::engine_svc::today(&crate::engine_svc::profile(c));
        let mut v = ai::state(c, &today)?;
        v["balance"] = crate::db::get_setting(c, "ai_balance")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        v["hasKey"] = json!(secrets::get("deepseek_api_key")?
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false));
        Ok(v)
    })
}
#[tauri::command]
pub fn ai_save(
    section: String,
    value: Value,
    app: AppHandle,
    st: State<'_, AppState>,
) -> Result<(), String> {
    with_db(&st, |c| match section.as_str() {
        "config" => ai::save_config(c, &value),
        "profile" => {ai::save_training(c,&value)?;crate::schedule::profile_changed(c)},
        "planner" => planning::switch(c, value.as_str().ok_or("Выбери автора")?).map(|_| ()),
        _ => Err("Неизвестная настройка".into()),
    })?;
    let _ = app.emit("data-changed", "ai");
    Ok(())
}
#[tauri::command]
pub fn ai_preview(
    kind: String,
    start: Option<String>,
    st: State<'_, AppState>,
) -> Result<Value, String> {
    with_db_mut(&st, |c| ai_context::preview(c, &kind, start.as_deref()))
}
#[tauri::command]
pub fn ai_prepare(
    snapshot_id: String,
    force: bool,
    st: State<'_, AppState>,
) -> Result<Value, String> {
    guard(&st)?;
    if secrets::get("deepseek_api_key")?
        .filter(|s| !s.trim().is_empty())
        .is_none()
    {
        return Err("Сначала сохрани ключ DeepSeek в настройках тренера".into());
    }
    with_db(&st, |c| ai::prepare(c, &snapshot_id, force))
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "Не удалось создать соединение".into())
}
#[tauri::command]
pub async fn ai_connect(st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    let key = secrets::get("deepseek_api_key")?
        .filter(|s| !s.trim().is_empty())
        .ok_or("Нет ключа DeepSeek")?;
    let r = client()?
        .get("https://api.deepseek.com/models")
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| "Не удалось связаться с DeepSeek. Проверь интернет".to_string())?;
    if !r.status().is_success() {
        return Err(format!(
            "DeepSeek: HTTP {}. Проверь ключ и баланс",
            r.status().as_u16()
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn ai_execute(job_id: String, st: State<'_, AppState>) -> Result<Value, String> {
    guard(&st)?;
    let key = secrets::get("deepseek_api_key")?
        .filter(|s| !s.trim().is_empty())
        .ok_or("Нет ключа DeepSeek")?;
    let client = client()?;
    let body = with_db(&st, |c| ai::take_request(c, &job_id))?;
    let result = async {
        let mut response = client
            .post("https://api.deepseek.com/chat/completions")
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                "Нет ответа DeepSeek. Возможна оплата запроса; повтор — только вручную".to_string()
            })?;
        if !response.status().is_success() {
            return Err(format!(
                "DeepSeek: HTTP {}. Проверь ключ, баланс или доступность сервиса",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Ответ прервался; возможна оплата запроса".to_string())?
        {
            if bytes.len() + chunk.len() > 2_000_000 {
                return Err("Ответ превышает допустимый размер".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| "Сервис вернул некорректный ответ".to_string())
    }
    .await;
    match result {
        Ok(v) => with_db(&st, |c| ai::finish(c, &job_id, &v)),
        Err(e) => {
            with_db(&st, |c| ai::fail(c, &job_id, &e, true))?;
            Err(e)
        }
    }
}
#[tauri::command]
pub fn ai_cancel(job_id: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| ai::cancel(c, &job_id))
}
#[tauri::command]
pub fn ai_accept(job_id: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db_mut(&st, |c| {
        let s = crate::engine_svc::summary(c)?;
        ai::accept_with_tier(
            c,
            &job_id,
            s["day"]["constraints"]["tier"].as_u64().map(|n| n as usize),
        )
    })?;
    let _ = app.emit("data-changed", "ai-plan");
    Ok(())
}

#[tauri::command]
pub async fn ai_balance(st: State<'_, AppState>) -> Result<Value, String> {
    guard(&st)?;
    let key = secrets::get("deepseek_api_key")?
        .filter(|s| !s.trim().is_empty())
        .ok_or("Сохрани API-ключ для проверки баланса")?;
    let mut response = client()?
        .get("https://api.deepseek.com/user/balance")
        .bearer_auth(&key)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| "Не удалось обновить баланс: проверь интернет".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Баланс не обновлён: DeepSeek HTTP {}. Проверь ключ и доступность сервиса",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Не удалось получить баланс полностью".to_string())?
    {
        if bytes.len() + chunk.len() > 65536 {
            return Err("Ответ о балансе слишком большой".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let raw: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "DeepSeek вернул некорректный ответ о балансе")?;
    let value = ai::normalize_balance(&raw, crate::store::now())?;
    if secrets::get("deepseek_api_key")?.as_deref() != Some(key.as_str()) {
        return Err("Ключ изменился во время проверки. Обнови баланс снова".into());
    }
    with_db(&st, |c| {
        if secrets::get("deepseek_api_key")?.as_deref() != Some(key.as_str()) {
            return Err("Ключ изменился во время проверки".into());
        }
        crate::db::set_setting(c, "ai_balance", &value.to_string())
    })?;
    Ok(value)
}
