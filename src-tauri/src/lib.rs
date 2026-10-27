//! Kouros desktop backend: data folder, encrypted database, secrets, Windows Hello.

mod db;
pub mod engine;
mod engine_svc;
mod manual_svc;
mod export;
mod freddy;
mod hello;
mod importer;
pub mod parse;
mod secrets;
mod store;
mod activity_svc;
mod training_svc;
mod map_tiles;
mod health_svc;
mod nutrition_svc;
mod psych_svc;
mod stress_svc;
mod food_est;
mod food_dishes;
mod norms;
mod muscle_svc;
mod meds_svc;
mod labs_svc;
mod exercise_svc;
mod shoe_svc;
mod drugs;
mod catalog_svc;
mod coach_svc;
mod sources;
mod hae;
mod plan_svc;
mod body_svc;
mod workouts_svc;
mod strava;
mod sync;

use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
pub struct AppState {
    data_dir: Mutex<Option<PathBuf>>,
    pub(crate) db: Mutex<Option<Connection>>,
    db_error: Mutex<Option<String>>,
    /// None until the first status call decides whether a Windows Hello check is needed.
    pub(crate) unlocked: Mutex<Option<bool>>,
    /// Parsed files waiting for the user to confirm the import.
    import: Mutex<Option<importer::Prepared>>,
    pub(crate) strava_busy: AtomicBool,
    pub(crate) freddy_busy: AtomicBool,
}

const SUBDIRS: [&str; 3] = ["files", "backups", "exports"];

fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(home.join("Kouros"))
}

fn init(app: &AppHandle) {
    let st = app.state::<AppState>();
    let root = match data_root(app) {
        Ok(r) => r,
        Err(e) => {
            *st.db_error.lock().unwrap() = Some(e);
            return;
        }
    };
    for sub in SUBDIRS {
        if let Err(e) = std::fs::create_dir_all(root.join(sub)) {
            *st.db_error.lock().unwrap() = Some(format!("Не удалось создать папку данных: {e}"));
            return;
        }
    }
    *st.data_dir.lock().unwrap() = Some(root.clone());
    match db::open(&root) {
        Ok(mut conn) => {
            if let Err(e) = db::auto_backup(&conn, &root) {
                db::log(&conn, "error", &e);
            }
            activity_svc::recount_once(&mut conn);
            *st.db.lock().unwrap() = Some(conn);
        }
        Err(e) => *st.db_error.lock().unwrap() = Some(e),
    }
}

fn lock_enabled(st: &AppState) -> bool {
    st.db
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| db::get_setting(c, "lock_enabled"))
        .map(|v| v == "1")
        .unwrap_or(true)
}

fn guard(st: &AppState) -> Result<(), String> {
    if *st.unlocked.lock().unwrap() == Some(true) {
        Ok(())
    } else {
        Err("Сначала войдите через Windows Hello".into())
    }
}

fn with_db<T>(st: &AppState, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    guard(st)?;
    let lock = st.db.lock().unwrap();
    let conn = lock.as_ref().ok_or("База данных не открыта")?;
    f(conn)
}

fn root(st: &AppState) -> Result<PathBuf, String> {
    st.data_dir.lock().unwrap().clone().ok_or_else(|| "Папка данных недоступна".to_string())
}

// ---------- commands ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    version: String,
    platform: String,
    data_dir: Option<String>,
    db_ok: bool,
    db_error: Option<String>,
    schema_version: i64,
    db_size: u64,
    cipher_version: Option<String>,
    hello: String,
    lock_enabled: bool,
    unlocked: bool,
}

#[tauri::command]
async fn app_status(app: AppHandle, st: State<'_, AppState>) -> Result<Status, String> {
    let hello = hello::availability();
    let lock_on = lock_enabled(&st);
    let unlocked = {
        let mut u = st.unlocked.lock().unwrap();
        if u.is_none() {
            *u = Some(!(lock_on && hello == "available"));
        }
        u.unwrap_or(false)
    };
    let data_dir = st.data_dir.lock().unwrap().clone();
    let (db_ok, schema_version, cipher_version) = match st.db.lock().unwrap().as_ref() {
        Some(c) => (true, db::schema_version(c), db::cipher_version(c)),
        None => (false, 0, None),
    };
    Ok(Status {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        db_size: data_dir.as_ref().map(|d| db::size_bytes(d)).unwrap_or(0),
        data_dir: data_dir.map(|d| d.to_string_lossy().to_string()),
        db_ok,
        db_error: st.db_error.lock().unwrap().clone(),
        schema_version,
        cipher_version,
        hello: hello.to_string(),
        lock_enabled: lock_on,
        unlocked,
    })
}

#[tauri::command]
async fn unlock(window: WebviewWindow, st: State<'_, AppState>) -> Result<bool, String> {
    if *st.unlocked.lock().unwrap() == Some(true) {
        return Ok(true);
    }
    let ok = hello::verify(&window, "Вход в Kouros")?;
    if ok {
        *st.unlocked.lock().unwrap() = Some(true);
    }
    Ok(ok)
}

#[tauri::command]
async fn set_lock_enabled(
    enabled: bool,
    window: WebviewWindow,
    st: State<'_, AppState>,
) -> Result<bool, String> {
    guard(&st)?;
    // Turning protection off must be confirmed by the owner.
    if !enabled && !hello::verify(&window, "Отключить вход по Windows Hello")? {
        return Ok(false);
    }
    with_db(&st, |c| db::set_setting(c, "lock_enabled", if enabled { "1" } else { "0" }))?;
    Ok(true)
}

#[tauri::command]
fn secret_status(st: State<'_, AppState>) -> Result<BTreeMap<String, bool>, String> {
    guard(&st)?;
    Ok(secrets::USER_SECRETS.iter().map(|n| (n.to_string(), secrets::has(n))).collect())
}

#[tauri::command]
fn secret_set(name: String, value: String, st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    if !secrets::is_user_secret(&name) {
        return Err("Неизвестный ключ".into());
    }
    let v = value.trim();
    if v.is_empty() {
        return Err("Пустое значение".into());
    }
    secrets::set(&name, v)?;
    with_db(&st, |c| {
        db::log(c, "secret", &format!("Сохранён ключ {name}"));
        Ok(())
    })
}

#[tauri::command]
fn secret_delete(name: String, st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    if !secrets::is_user_secret(&name) {
        return Err("Неизвестный ключ".into());
    }
    secrets::delete(&name)?;
    with_db(&st, |c| {
        db::log(c, "secret", &format!("Удалён ключ {name}"));
        Ok(())
    })
}

#[tauri::command]
fn sources_list(st: State<'_, AppState>) -> Result<Vec<db::Source>, String> {
    with_db(&st, db::sources)
}

#[tauri::command]
fn backups_list(st: State<'_, AppState>) -> Result<Vec<db::BackupInfo>, String> {
    guard(&st)?;
    Ok(db::list_backups(&root(&st)?))
}

#[tauri::command]
async fn backup_now(st: State<'_, AppState>) -> Result<String, String> {
    let r = root(&st)?;
    with_db(&st, |c| db::backup_now(c, &r))
}

#[tauri::command]
fn open_data_dir(app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    let r = root(&st)?;
    app.opener()
        .open_path(r.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn recovery_key(window: WebviewWindow, st: State<'_, AppState>) -> Result<Option<String>, String> {
    guard(&st)?;
    if !hello::verify(&window, "Показать ключ восстановления Kouros")? {
        return Ok(None);
    }
    secrets::db_key(false).map(Some)
}

// ---------- stage 2: workouts, import, Strava, freddy ----------

fn with_db_mut<T>(st: &AppState, f: impl FnOnce(&mut Connection) -> Result<T, String>) -> Result<T, String> {
    guard(st)?;
    let mut lock = st.db.lock().unwrap();
    let conn = lock.as_mut().ok_or("База данных не открыта")?;
    f(conn)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn activities_list(
    sport: Option<String>,
    days: Option<i64>,
    query: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    st: State<'_, AppState>,
) -> Result<store::ActivityPage, String> {
    let f = store::ListFilter { sport: sport.as_deref(), days, query: query.as_deref(), sort: sort.as_deref() };
    with_db(&st, |c| store::list_activities(c, &f, limit.unwrap_or(50).clamp(1, 500), offset.unwrap_or(0)))
}

#[tauri::command]
fn activities_stats(st: State<'_, AppState>) -> Result<store::ActivityStats, String> {
    with_db(&st, store::activity_stats)
}

#[tauri::command]
async fn import_preview(paths: Vec<String>, st: State<'_, AppState>) -> Result<importer::Preview, String> {
    let prepared = with_db(&st, |c| importer::prepare(c, &paths))?;
    let preview = importer::preview(&prepared);
    *st.import.lock().unwrap() = Some(prepared);
    Ok(preview)
}

#[tauri::command]
async fn import_commit(merge: bool, app: AppHandle, st: State<'_, AppState>) -> Result<importer::ImportResult, String> {
    let prepared = st.import.lock().unwrap().take().ok_or("Нет подготовленного импорта — выбери файлы заново")?;
    let r = root(&st)?;
    let res = with_db_mut(&st, |c| importer::commit(c, &r, prepared, merge))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "import");
    Ok(res)
}

#[tauri::command]
fn import_cancel(st: State<'_, AppState>) {
    *st.import.lock().unwrap() = None;
}

#[tauri::command]
fn import_history(st: State<'_, AppState>) -> Result<Vec<store::ImportLog>, String> {
    with_db(&st, store::import_history)
}

#[tauri::command]
async fn export_activities(
    days: Option<i64>,
    format: String,
    app: AppHandle,
    st: State<'_, AppState>,
) -> Result<export::ExportResult, String> {
    let r = root(&st)?;
    let res = with_db(&st, |c| export::export(c, &r, days, &format))?;
    let _ = app.opener().open_path(res.folder.clone(), None::<&str>);
    Ok(res)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceStatus {
    keys: bool,
    connected: bool,
    athlete: Option<String>,
    last_ok: Option<String>,
    last_error: Option<String>,
    extra: Option<String>,
    busy: bool,
}

#[tauri::command]
fn strava_status(st: State<'_, AppState>) -> Result<SourceStatus, String> {
    let busy = st.strava_busy.load(std::sync::atomic::Ordering::SeqCst);
    with_db(&st, |c| {
        let s = store::sync_state(c, "strava");
        Ok(SourceStatus {
            keys: secrets::has("strava_client_id") && secrets::has("strava_client_secret"),
            connected: strava::is_connected(),
            athlete: db::get_setting(c, "strava_athlete"),
            last_ok: s.last_ok,
            last_error: s.last_error,
            extra: s.extra,
            busy,
        })
    })
}

#[tauri::command]
async fn strava_connect(app: AppHandle, st: State<'_, AppState>) -> Result<String, String> {
    guard(&st)?;
    let (url, listener, state) = strava::begin_auth()?;
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("Не удалось открыть браузер: {e}"))?;
    let name = strava::finish_auth(listener, state).await?;
    with_db(&st, |c| {
        db::set_setting(c, "strava_athlete", &name)?;
        store::set_source_connected(c, "strava", true);
        db::log(c, "strava", &format!("Strava подключена: {name}"));
        Ok(())
    })?;
    Ok(name)
}

#[tauri::command]
async fn strava_sync(full: bool, app: AppHandle, st: State<'_, AppState>) -> Result<sync::StravaSync, String> {
    guard(&st)?;
    sync::strava_sync(&app, full).await
}

#[tauri::command]
fn strava_disconnect(st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    strava::disconnect()?;
    with_db(&st, |c| {
        store::set_source_connected(c, "strava", false);
        db::log(c, "strava", "Strava отключена");
        Ok(())
    })
}

#[tauri::command]
fn freddy_status(st: State<'_, AppState>) -> Result<SourceStatus, String> {
    let busy = st.freddy_busy.load(std::sync::atomic::Ordering::SeqCst);
    with_db(&st, |c| {
        let s = store::sync_state(c, "freddy");
        Ok(SourceStatus {
            keys: secrets::has("freddy_api_key"),
            connected: s.last_ok.is_some(),
            athlete: None,
            last_ok: s.last_ok,
            last_error: s.last_error,
            extra: s.cursor,
            busy,
        })
    })
}

#[tauri::command]
async fn freddy_sync(full: bool, app: AppHandle, st: State<'_, AppState>) -> Result<sync::FreddySync, String> {
    guard(&st)?;
    freddy::check().await?;
    sync::freddy_sync(&app, full).await
}

/// Metrics shown on the Health tab, in display order.
const HEALTH_METRICS: [&str; 16] = [
    "body_mass",
    "body_fat_pct",
    "lean_body_mass",
    "resting_heart_rate_merged_avg",
    "hrv_rmssd_merged_avg",
    "vo2_max",
    "hr_recovery_1min",
    "respiratory_rate_merged_avg",
    "sleeping_breathing_disturbances",
    "sleep_analysis_deep_seconds",
    "sleep_analysis_rem_seconds",
    "sleep_analysis_core_seconds",
    "steps_total",
    "active_energy_total",
    "dietary_energy_total",
    "dietary_water_total",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthOverview {
    stats: store::HealthStats,
    latest: Vec<store::MetricLatest>,
}

#[tauri::command]
fn health_overview(st: State<'_, AppState>) -> Result<HealthOverview, String> {
    with_db(&st, |c| Ok(HealthOverview { stats: store::health_stats(c)?, latest: store::latest_metrics(c, &HEALTH_METRICS)? }))
}

// ---------- stage 3: engine ----------

#[tauri::command]
async fn engine_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, engine_svc::summary)
}

#[tauri::command]
fn profile_get(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| {
        let p = engine_svc::profile(c);
        let mut v = engine_svc::profile_json(&p);
        v["hrRestAuto"] = serde_json::json!(engine_svc::resting_hr(c, &engine::Profile { hr_rest: None, ..p.clone() }));
        Ok(v)
    })
}

#[tauri::command]
fn profile_set(profile: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| engine_svc::save_profile(c, &profile))
}

// ---------- stage 4: workout page, effort, sets, shoes ----------

#[tauri::command]
fn activity_get(id: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| activity_svc::detail(c, &id))
}

#[tauri::command]
fn activity_update(id: String, rpe: Option<f64>, notes: Option<String>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| activity_svc::update(c, &id, rpe, notes))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "activity");
    Ok(())
}

#[tauri::command]
fn strength_sets_save(id: String, sets: Vec<serde_json::Value>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db_mut(&st, |c| activity_svc::save_sets(c, &id, &sets))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "activity");
    Ok(())
}

#[tauri::command]
fn ofp_save(id: String, blocks: Vec<serde_json::Value>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db_mut(&st, |c| activity_svc::save_ofp(c, &id, &blocks))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "activity");
    Ok(())
}

#[tauri::command]
fn ofp_last(id: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(activity_svc::ofp_last(c, &id)))
}

#[tauri::command]
fn device_change_decide(changes: Vec<serde_json::Value>, reset: bool, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| {
        sources::device_decide(c, &changes, reset)?;
        sources::install_view(c)
    })?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "sources");
    Ok(())
}

#[tauri::command]
fn manual_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(manual_svc::state(c)))
}

#[tauri::command]
fn manual_save(date: String, values: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| manual_svc::save(c, &date, &values))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "manual");
    Ok(())
}

#[tauri::command]
fn complexes_get(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(exercise_svc::complexes(c)))
}

#[tauri::command]
fn complexes_save(complexes: Vec<serde_json::Value>, custom: Vec<serde_json::Value>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| exercise_svc::save_complexes(c, &complexes, &custom))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "complexes");
    Ok(())
}

#[tauri::command]
fn complexes_import(text: String, app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (c, e) = with_db(&st, |c| exercise_svc::import_complexes(c, &text))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "complexes");
    Ok(serde_json::json!({"complexes": c, "exercises": e}))
}

#[tauri::command]
fn exercises_recent(st: State<'_, AppState>) -> Result<Vec<String>, String> {
    with_db(&st, |c| Ok(activity_svc::recent_exercises(c)))
}

#[tauri::command]
fn exercise_library() -> serde_json::Value {
    exercise_svc::list()
}

#[tauri::command]
fn bands_get(st: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    with_db(&st, |c| Ok(exercise_svc::bands(c)))
}

#[tauri::command]
fn bands_set(bands: Vec<serde_json::Value>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| exercise_svc::save_bands(c, &bands))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "bands");
    Ok(())
}

/// Opens a review page of a shoe model in the browser (only http/https links).
#[tauri::command]
fn open_link(url: String, app: AppHandle) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Это не ссылка на сайт".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("Не удалось открыть браузер: {e}"))
}

#[tauri::command]
fn shoe_advice(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| shoe_svc::advice(c))
}

#[tauri::command]
fn shoe_conditions_set(temp: String, surface: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| {
        let today = engine_svc::today(&engine_svc::profile(c));
        shoe_svc::set_conditions(c, &today, &temp, &surface)
    })?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "shoes");
    Ok(())
}

#[tauri::command]
fn exercises_list(st: State<'_, AppState>) -> Result<Vec<String>, String> {
    with_db(&st, |c| Ok(activity_svc::exercises(c)))
}

#[tauri::command]
fn shoes_get(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| activity_svc::shoes(c, engine_svc::profile(c).tz_offset_s))
}

#[tauri::command]
fn shoe_runs(id: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| activity_svc::shoe_runs(c, &id))
}

#[tauri::command]
fn shoes_set(shoes: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| activity_svc::save_shoes(c, &shoes))
}

#[tauri::command]
fn activity_set_shoe(id: String, shoe: Option<String>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| activity_svc::set_shoe(c, &id, shoe))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "activity");
    Ok(())
}

#[tauri::command]
async fn training_overview(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, training_svc::overview)
}

#[tauri::command]
fn records_extra(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, training_svc::records)
}

#[tauri::command]
fn setting_list_set(key: String, value: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    if !matches!(key.as_str(), "records_hidden" | "strength_manual") {
        return Err("Неизвестная настройка".into());
    }
    with_db(&st, |c| training_svc::set_list(c, &key, &value))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MapCache {
    enabled: bool,
    tiles: u64,
    bytes: u64,
}

#[tauri::command]
fn map_cache_info(st: State<'_, AppState>) -> Result<MapCache, String> {
    let r = root(&st)?;
    let enabled = with_db(&st, |c| Ok(db::get_setting(c, "map_online").map(|v| v == "1").unwrap_or(false)))?;
    let (tiles, bytes) = map_tiles::cache_size(&r);
    Ok(MapCache { enabled, tiles, bytes })
}

#[tauri::command]
fn map_set_enabled(enabled: bool, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| db::set_setting(c, "map_online", if enabled { "1" } else { "0" }))
}

#[tauri::command]
fn map_cache_clear(st: State<'_, AppState>) -> Result<(), String> {
    guard(&st)?;
    let dir = map_tiles::cache_dir(&root(&st)?);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Не удалось очистить: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn health_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, health_svc::summary)
}

#[tauri::command]
fn health_day(date: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| {
        let mut v = health_svc::day(c, &date)?;
        v["readiness"] = engine_svc::readiness_day(c, &date)?;
        Ok(v)
    })
}

#[tauri::command]
fn nutrition_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, nutrition_svc::summary)
}

#[tauri::command]
fn nutrition_day(date: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| nutrition_svc::day(c, &date))
}

#[tauri::command]
fn psych_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, psych_svc::summary)
}

#[tauri::command]
fn survey_save(kind: String, answers: serde_json::Value, date: Option<String>, app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let r = with_db(&st, |c| psych_svc::save(c, &kind, &answers, date.as_deref()))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "survey");
    Ok(r)
}

#[tauri::command]
fn survey_delete(kind: String, date: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| psych_svc::delete(c, &kind, &date))
}

#[tauri::command]
fn meds_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| {
        let today = engine_svc::today(&engine_svc::profile(c));
        let food = nutrition_svc::food7(c);
        meds_svc::summary(c, &today, &food)
    })
}

#[tauri::command]
fn med_save(med: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<i64, String> {
    let id = with_db(&st, |c| meds_svc::save(c, &med))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "meds");
    Ok(id)
}

#[tauri::command]
fn med_delete(id: i64, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| meds_svc::delete(c, id))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "meds");
    Ok(())
}

#[tauri::command]
fn med_take(id: i64, date: String, servings: f64, time: Option<String>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| {
        // «08:15» on that date (local) → unix seconds.
        let ts = time.as_deref().and_then(|t| {
            let (h, m) = t.split_once(':')?;
            let (h, m): (i64, i64) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
            let tz = engine_svc::profile(c).tz_offset_s;
            Some(health_svc::iso(&format!("{date}T00:00:00Z"))? - tz + h * 3600 + m * 60)
        });
        meds_svc::take(c, id, &date, servings, ts)
    })?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "meds");
    Ok(())
}

#[tauri::command]
fn catalog_brands() -> serde_json::Value {
    catalog_svc::brands()
}

#[tauri::command]
fn catalog_search(brand: Option<String>, kind: Option<String>, q: String) -> Vec<serde_json::Value> {
    catalog_svc::search(brand.as_deref().filter(|b| !b.is_empty()), kind.as_deref().filter(|k| !k.is_empty()), &q, 200)
}

#[tauri::command]
fn catalog_product(id: String) -> Option<serde_json::Value> {
    catalog_svc::product(&id)
}

#[tauri::command]
async fn catalog_online(q: String, st: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    if !with_db(&st, |c| Ok(catalog_svc::online_enabled(c)))? {
        return Err("Поиск в интернете выключен в настройках".into());
    }
    catalog_svc::search_online(&q).await
}

#[tauri::command]
fn catalog_label(text: String) -> Result<serde_json::Value, String> {
    catalog_svc::from_label(&text).ok_or_else(|| "Не нашлась строка «[ДОБАВКА] …» с составом".to_string())
}

#[tauri::command]
fn drug_search(q: String, st: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    with_db(&st, |c| Ok(catalog_svc::drug_search(c, &q)))
}

#[tauri::command]
async fn drug_online(inn: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if !with_db(&st, |c| Ok(catalog_svc::online_enabled(c)))? {
        return Err("Поиск в интернете выключен в настройках".into());
    }
    catalog_svc::drug_online(&inn).await
}

#[tauri::command]
fn drug_registry_info(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| {
        let mut v = catalog_svc::registry_info(c);
        v["online"] = serde_json::json!(catalog_svc::online_enabled(c));
        Ok(v)
    })
}

#[tauri::command]
async fn drug_registry_update(st: State<'_, AppState>) -> Result<usize, String> {
    let (name, rows) = catalog_svc::registry_download().await?;
    with_db_mut(&st, |c| catalog_svc::registry_store(c, &name, &rows))
}

#[tauri::command]
fn deficit_snooze(key: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| nutrition_svc::deficit_snooze(c, &key))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "nutrition");
    Ok(())
}

#[tauri::command]
fn labs_parse(path: String) -> Result<serde_json::Value, String> {
    labs_svc::parse_pdf(&path)
}

#[tauri::command]
fn labs_parse_text(text: String) -> Result<serde_json::Value, String> {
    labs_svc::parse_text(&text)
}

#[tauri::command]
fn labs_save(report: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<i64, String> {
    let files = root(&st)?.join("files");
    let id = with_db_mut(&st, |c| labs_svc::save(c, &files, &report))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "labs");
    Ok(id)
}

#[tauri::command]
fn labs_summary(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| {
        let p = engine_svc::profile(c);
        labs_svc::summary(c, &engine_svc::today(&p))
    })
}

#[tauri::command]
fn labs_delete(id: i64, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    let files = root(&st)?.join("files");
    with_db(&st, |c| labs_svc::delete(c, &files, id))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "labs");
    Ok(())
}

#[tauri::command]
fn labs_open(id: i64, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    let files = root(&st)?.join("files");
    let f: Option<String> = with_db(&st, |c| Ok(c.query_row("SELECT file FROM lab_reports WHERE id = ?1", [id], |r| r.get(0)).ok().flatten()))?;
    let f = f.ok_or("У этого отчёта нет файла")?;
    app.opener().open_path(files.join(f).to_string_lossy().to_string(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn meds_enabled(st: State<'_, AppState>) -> Result<bool, String> {
    with_db(&st, |c| Ok(meds_svc::enabled(c)))
}

#[tauri::command]
fn meds_enabled_set(on: bool, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| db::set_setting(c, "meds_enabled", if on { "1" } else { "0" }))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "meds");
    Ok(())
}

#[tauri::command]
fn supp_online_set(on: bool, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| db::set_setting(c, "supp_online", if on { "1" } else { "0" }))
}

#[tauri::command]
fn workouts_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(workouts_svc::state(c)))
}

#[tauri::command]
fn workouts_save(data: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db_mut(&st, |c| workouts_svc::save_settings(c, &data))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn dups_pending(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(serde_json::json!(workouts_svc::pending(c))))
}

#[tauri::command]
fn dup_decide(group: String, keep: Option<String>, remember: bool, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::decide(c, &group, keep.as_deref(), remember))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn workout_choose(id: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::choose(c, &id))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn strava_gone(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(workouts_svc::gone(c)))
}

#[tauri::command]
fn strava_gone_decide(id: String, delete: bool, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::gone_decide(c, &id, delete))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn workouts_delete(ids: Vec<String>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::delete(c, &ids))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn workout_restore(id: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::restore(c, &id))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "workouts");
    Ok(())
}

#[tauri::command]
fn workout_purge(id: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| workouts_svc::purge(c, &id))
}

#[tauri::command]
fn workouts_deleted(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(serde_json::json!(workouts_svc::deleted(c))))
}

#[tauri::command]
fn body_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| body_svc::state(c))
}

#[tauri::command]
fn checkin_save(id: String, data: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| body_svc::save_checkin(c, &id, &data))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "checkin");
    Ok(())
}

#[tauri::command]
fn pain_mark(data: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| body_svc::pain_mark(c, &data))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "pain");
    Ok(())
}

#[tauri::command]
fn pain_delete(id: i64, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| body_svc::pain_delete(c, id))
}

#[tauri::command]
fn return_start(area: String, stage: usize, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| body_svc::return_start(c, &area, stage))
}

#[tauri::command]
fn return_stop(st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| body_svc::return_stop(c))
}

#[tauri::command]
fn plan_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| plan_svc::state(c))
}

#[tauri::command]
fn plan_decide(id: i64, accept: bool, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| plan_svc::decide(c, id, accept))
}

#[tauri::command]
fn plan_check_choice(date: String, items: serde_json::Value, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| plan_svc::check_choice(c, &date, &items))
}

#[tauri::command]
fn plan_choose(date: String, items: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let v = with_db_mut(&st, |c| plan_svc::choose(c, &date, &items))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "plan");
    Ok(v)
}

#[tauri::command]
fn plan_confirm(date: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| plan_svc::confirm(c, &date))
}

#[tauri::command]
fn plan_set_day(date: String, items: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| plan_svc::set_day(c, &date, &items))
}

#[tauri::command]
fn plan_reset_day(date: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| plan_svc::reset_day(c, &date))
}

#[tauri::command]
fn plan_template_save(days: serde_json::Value, rebuild: bool, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| plan_svc::save_template_days(c, &days, rebuild))
}

#[tauri::command]
fn sources_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(sources::state(c)))
}

#[tauri::command]
fn sources_preview(group: String, provider: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(sources::preview(c, &group, &provider)))
}

#[tauri::command]
fn sources_set(group: String, provider: String, mode: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| sources::set(c, &group, &provider, &mode))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "sources");
    Ok(())
}

#[tauri::command]
fn sources_set_all(provider: String, mode: String, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| sources::set_all(c, &provider, &mode))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "sources");
    Ok(())
}

#[tauri::command]
fn hae_state(app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| Ok(hae::state(&app, c)))
}

#[tauri::command]
fn hae_set(patch: serde_json::Value, app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| hae::set(c, &patch))?;
    if patch.get("enabled").is_some() || patch.get("port").is_some() || patch.get("newToken").is_some() {
        hae::restart(&app);
    }
    with_db(&st, |c| Ok(hae::state(&app, c)))
}

fn hae_err(e: String) -> String {
    if e == "locked" { "Kouros заблокирован — войди через Windows Hello и повтори".into() } else { e }
}

#[tauri::command]
async fn hae_import_file(path: String, app: AppHandle, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    guard(&st)?;
    let a = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || hae::import_path(&a, std::path::Path::new(&path), "file"))
        .await
        .map_err(|e| e.to_string())?
        .map_err(hae_err)?;
    Ok(serde_json::json!({"samples": res.samples, "first": res.first, "last": res.last, "metrics": res.metrics, "unknown": res.unknown.keys().collect::<Vec<_>>()}))
}

#[tauri::command]
async fn hae_scan(app: AppHandle, st: State<'_, AppState>) -> Result<usize, String> {
    guard(&st)?;
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        hae::drain(&a);
        hae::scan_folder(&a)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(hae_err)
}

#[tauri::command]
fn sources_map_set(provider: String, canon: String, metric: Option<String>, app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| sources::map_set(c, &provider, &canon, metric.as_deref()))?;
    let _ = tauri::Emitter::emit(&app, "data-changed", "sources");
    Ok(())
}

#[tauri::command]
fn coach_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| coach_svc::state(c))
}

#[tauri::command]
fn coach_profile_save(profile: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| {
        coach_svc::save_profile(c, &profile)?;
        plan_svc::on_profile_saved(c)
    })
}

#[tauri::command]
fn coach_notes_save(text: String, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| coach_svc::save_notes(c, &text))
}

#[tauri::command]
fn coach_day(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| coach_svc::day(c))
}

#[tauri::command]
fn coach_week(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db_mut(&st, |c| coach_svc::week(c))
}

#[tauri::command]
fn coach_package(report_id: Option<i64>, question: Option<String>, st: State<'_, AppState>) -> Result<String, String> {
    with_db_mut(&st, |c| coach_svc::package(c, report_id, question.as_deref()))
}

#[tauri::command]
fn coach_save_answer(parent: Option<i64>, text: String, st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_db(&st, |c| coach_svc::save_answer(c, parent, &text))
}

#[tauri::command]
fn coach_delete(id: i64, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| coach_svc::delete(c, id))
}

#[tauri::command]
fn habit_mark(date: String, key: String, val: Option<i64>, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| health_svc::mark(c, &date, &key, val))
}

#[tauri::command]
fn health_setting_set(key: String, value: serde_json::Value, st: State<'_, AppState>) -> Result<(), String> {
    with_db(&st, |c| health_svc::set(c, &key, &value))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .register_asynchronous_uri_scheme_protocol("kmap", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            let path = req.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(map_tiles::serve(app, path).await);
            });
        })
        .setup(|app| {
            init(app.handle());
            sync::start_background(app.handle().clone());
            hae::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            unlock,
            set_lock_enabled,
            secret_status,
            secret_set,
            secret_delete,
            sources_list,
            backups_list,
            backup_now,
            open_data_dir,
            recovery_key,
            activities_list,
            activities_stats,
            import_preview,
            import_commit,
            import_cancel,
            import_history,
            export_activities,
            strava_status,
            strava_connect,
            strava_sync,
            strava_disconnect,
            freddy_status,
            freddy_sync,
            health_overview,
            engine_summary,
            profile_get,
            profile_set,
            activity_get,
            activity_update,
            strength_sets_save,
            ofp_save,
            exercises_recent,
            ofp_last,
            complexes_get,
            manual_state,
            manual_save,
            device_change_decide,
            complexes_save,
            complexes_import,
            exercises_list,
            shoe_advice,
            open_link,
            shoe_conditions_set,
            exercise_library,
            bands_get,
            bands_set,
            shoes_get,
            shoes_set,
            shoe_runs,
            activity_set_shoe,
            training_overview,
            records_extra,
            setting_list_set,
            map_cache_info,
            map_cache_clear,
            map_set_enabled,
            health_summary,
            health_day,
            nutrition_summary,
            nutrition_day,
            psych_summary,
            survey_save,
            survey_delete,
            habit_mark,
            health_setting_set,
            meds_summary,
            med_save,
            med_delete,
            med_take,
            catalog_brands,
            catalog_search,
            catalog_product,
            catalog_online,
            catalog_label,
            drug_search,
            drug_online,
            drug_registry_info,
            drug_registry_update,
            supp_online_set,
            meds_enabled,
            labs_parse,
            deficit_snooze,
            labs_parse_text,
            labs_save,
            labs_summary,
            labs_delete,
            labs_open,
            meds_enabled_set,
            coach_state,
            coach_profile_save,
            coach_notes_save,
            coach_day,
            coach_week,
            coach_package,
            coach_save_answer,
            coach_delete,
            sources_state,
            sources_preview,
            sources_set,
            sources_map_set,
            sources_set_all,
            hae_state,
            hae_set,
            hae_import_file,
            hae_scan,
            plan_state,
            plan_decide,
            plan_check_choice,
            plan_choose,
            plan_confirm,
            plan_set_day,
            plan_reset_day,
            plan_template_save,
            body_state,
            checkin_save,
            pain_mark,
            pain_delete,
            return_start,
            return_stop,
            workouts_state,
            workouts_save,
            dups_pending,
            dup_decide,
            workout_choose,
            workouts_delete,
            strava_gone,
            strava_gone_decide,
            workout_restore,
            workout_purge,
            workouts_deleted,
        ])
        .run(tauri::generate_context!())
        .expect("Kouros не смог запуститься");
}
