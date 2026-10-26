//! Sync jobs that combine network calls with database writes, plus the
//! background loop that runs them every 20 minutes.

use crate::parse::time;
use crate::{freddy, store, strava, AppState};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const EVERY: Duration = Duration::from_secs(20 * 60);

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>) -> Result<T, String> {
    let st = app.state::<AppState>();
    let mut lock = st.db.lock().unwrap();
    let conn = lock.as_mut().ok_or("База данных не открыта")?;
    f(conn)
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StravaSync {
    pub added: usize,
    pub merged: usize,
    pub streams: usize,
    pub streams_left: usize,
    pub rate_limited: bool,
    /// Workouts changed in Strava after they were loaded (1.3.2).
    pub updated: usize,
    /// Workouts of the last week that are gone from Strava (waiting for the athlete's answer).
    pub gone: usize,
    /// No workouts yet and no explicit choice — the UI should ask first.
    pub needs_choice: bool,
}

struct Busy<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub async fn strava_sync(app: &AppHandle, full: bool) -> Result<StravaSync, String> {
    if !strava::is_connected() {
        return Err("Strava не подключена".into());
    }
    let st = app.state::<AppState>();
    if st.strava_busy.swap(true, Ordering::SeqCst) {
        return Err("Синхронизация Strava уже идёт".into());
    }
    let _busy = Busy(&st.strava_busy);
    let res = strava_sync_inner(app, full).await;
    match &res {
        Ok(r) if !r.needs_choice => {
            let extra = format!("{{\"streamsLeft\":{}}}", r.streams_left);
            let _ = with_conn(app, |c| {
                store::set_sync_ok(c, "strava", None, Some(&extra));
                Ok(())
            });
        }
        Ok(_) => {}
        Err(e) => {
            let _ = with_conn(app, |c| {
                store::set_sync_error(c, "strava", e);
                Ok(())
            });
        }
    }
    let _ = app.emit("data-changed", "strava");
    res
}

async fn strava_sync_inner(app: &AppHandle, full: bool) -> Result<StravaSync, String> {
    let mut out = StravaSync::default();
    let (latest, count) = with_conn(app, |c| Ok((store::latest_strava_start(c), store::count_activities(c))))?;
    let after = match (full, latest) {
        (true, _) => 0,
        (false, Some(t)) => t - 86400,
        (false, None) if count > 0 => 0,
        (false, None) => {
            out.needs_choice = true;
            return Ok(out);
        }
    };
    let (list, limited) = strava::list_after(after).await?;
    out.rate_limited = limited;
    with_conn(app, |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        for (sid, p) in &list {
            if store::exists_strava(&tx, *sid).is_some() {
                continue;
            }
            if let Some((id, had)) = store::find_by_time(&tx, p.start_utc, p.elapsed_s) {
                if store::merge_activity(&tx, &id, Some(*sid), None, p, had)? {
                    out.merged += 1;
                }
                continue;
            }
            let id = store::activity_id_for(Some(*sid), p);
            store::insert_activity(&tx, &id, Some(*sid), "strava_api", None, p)?;
            out.added += 1;
        }
        tx.commit().map_err(|e| e.to_string())
    })?;
    // 1.3.2: the last 7 days again — a distance corrected, a crop, a new name or sport in Strava
    // keeps the same workout id, so it would never come back otherwise.
    if !out.rate_limited {
        let since = store::now() - 7 * 86400;
        let (recent, limited) = if after <= since { (list.clone(), false) } else { strava::list_after(since).await? };
        out.rate_limited |= limited;
        let (updated, gone) = with_conn(app, |c| {
            let mut n: usize = 0;
            for (sid, p) in &recent {
                if let Some((id, _)) = store::exists_strava(c, *sid) {
                    if store::update_from_strava(c, &id, p)? {
                        n += 1;
                    }
                }
            }
            // Gone from Strava: only when the list is complete.
            let mut gone: usize = 0;
            if !limited {
                let ids: std::collections::BTreeSet<i64> = recent.iter().map(|x| x.0).collect();
                let missing: Vec<_> = store::strava_since(c, since + 3600).into_iter().filter(|x| !ids.contains(&x.1)).collect();
                gone = crate::workouts_svc::mark_gone(c, &missing);
            }
            Ok((n, gone))
        })?;
        out.updated = updated;
        out.gone = gone;
        if updated > 0 {
            let _ = with_conn(app, |c| {
                crate::workouts_svc::today_changed(c);
                Ok(())
            });
        }
    }
    // Sample streams: newest first, a limited number per run.
    if !out.rate_limited {
        let todo = with_conn(app, |c| Ok(store::strava_missing_streams(c, strava::streams_per_run())))?;
        for (id, sid) in todo {
            match strava::streams(sid).await? {
                strava::Fetch::RateLimited => {
                    out.rate_limited = true;
                    break;
                }
                strava::Fetch::Ok(s) => {
                    with_conn(app, |c| store::set_streams(c, &id, &s))?;
                    out.streams += 1;
                }
            }
        }
    }
    out.streams_left = with_conn(app, |c| Ok(store::strava_missing_streams(c, 100_000).len()))?;
    let _ = with_conn(app, |c| crate::workouts_svc::after_sync(c));
    Ok(out)
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FreddySync {
    pub rows: usize,
    pub pages: usize,
    pub last_date: Option<String>,
}

const WORKOUT_METRICS: &str = "workout_activity_type,workout_duration,workout_distance,workout_active_energy,workout_elevation_ascended,workout_elevation_descended,workout_indoor";

pub async fn freddy_sync(app: &AppHandle, full: bool) -> Result<FreddySync, String> {
    let st = app.state::<AppState>();
    if st.freddy_busy.swap(true, Ordering::SeqCst) {
        return Err("Синхронизация freddy уже идёт".into());
    }
    let _busy = Busy(&st.freddy_busy);
    let res = freddy_sync_inner(app, full).await;
    match &res {
        Ok(r) => {
            let _ = with_conn(app, |c| {
                store::set_sync_ok(c, "freddy", r.last_date.as_deref(), None);
                Ok(())
            });
        }
        Err(e) => {
            let _ = with_conn(app, |c| {
                store::set_sync_error(c, "freddy", e);
                Ok(())
            });
        }
    }
    let _ = app.emit("data-changed", "freddy");
    res
}

async fn freddy_sync_inner(app: &AppHandle, full: bool) -> Result<FreddySync, String> {
    let last = with_conn(app, |c| Ok(store::sync_state(c, "freddy").cursor))?;
    // Re-read the last 3 days: late-arriving sleep and nutrition entries.
    let from = if full {
        None
    } else {
        last.as_deref().and_then(|d| {
            let y = d.get(0..4)?.parse().ok()?;
            let m = d.get(5..7)?.parse().ok()?;
            let dd = d.get(8..10)?.parse().ok()?;
            Some(time::ymd(time::unix(y, m, dd, 0, 0, 0) - 3 * 86400, 0))
        })
    };
    let mut out = FreddySync { last_date: last.clone(), ..Default::default() };
    let mut cursor: Option<String> = None;
    for _ in 0..500 {
        let page = freddy::page(from.as_deref(), cursor.as_deref(), None).await?;
        out.pages += 1;
        if let Some(d) = &page.max_date {
            if out.last_date.as_deref().map(|l| d.as_str() > l).unwrap_or(true) {
                out.last_date = Some(d.clone());
            }
        }
        if !page.rows.is_empty() {
            out.rows += with_conn(app, |c| store::upsert_metrics(c, &page.rows))?;
        }
        match page.next {
            Some(n) if page.seen > 0 => cursor = Some(n),
            _ => break,
        }
    }
    // Sleep stages and HRV samples: the whole history once, then the recent days.
    // "3" = full history fetched with pulse and steps (0.10.0); "2" was sleep + HRV only.
    let backfilled = with_conn(app, |c| Ok(crate::db::get_setting(c, "raw_backfill").as_deref() == Some("3")))?;
    let since = if backfilled {
        from.clone().unwrap_or_else(|| time::ymd(crate::store::now() - 14 * 86400, 0))
    } else {
        "2015-01-01".to_string()
    };
    let mut ok = true;
    for m in ["sleep_analysis_raw", "hrv_sdnn_raw", "heart_rate_raw", "steps_raw"] {
        match freddy::raw_rows(m, &since).await {
            Ok(rows) if !rows.is_empty() => {
                let _ = with_conn(app, |c| store::upsert_metrics(c, &rows));
            }
            Ok(_) => {}
            Err(_) => ok = false,
        }
    }
    if ok && !backfilled {
        let _ = with_conn(app, |c| crate::db::set_setting(c, "raw_backfill", "3"));
    }
    // A source newly connected in freddy (Garmin, Suunto, Fitbit…): its whole history once.
    let (provs, done): (Vec<String>, Vec<String>) = with_conn(app, |c| {
        let p: Vec<String> = crate::sources::catalog(c).into_iter().map(|x| x.provider).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let d: Vec<String> = crate::db::get_setting(c, "provider_backfill").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Ok((p, d))
    })?;
    let mut done = done;
    let todo: Vec<String> = provs.into_iter().filter(|p| !done.contains(p) && p != crate::sources::MANUAL).collect();
    for p in todo {
        let mut cursor: Option<String> = None;
        let mut complete = false;
        for _ in 0..500 {
            let Ok(page) = freddy::page(None, cursor.as_deref(), Some(&p)).await else { break };
            out.pages += 1;
            if !page.rows.is_empty() {
                out.rows += with_conn(app, |c| store::upsert_metrics(c, &page.rows))?;
            }
            match page.next {
                Some(n) if page.seen > 0 => cursor = Some(n),
                _ => {
                    complete = true;
                    break;
                }
            }
        }
        if complete {
            done.push(p);
            let v = serde_json::to_string(&done).unwrap_or_default();
            let _ = with_conn(app, |c| crate::db::set_setting(c, "provider_backfill", &v));
        }
    }
    // Workouts from freddy (0.17): summaries for the whole history once, heart rate of recent ones.
    let wmode = with_conn(app, |c| Ok(crate::workouts_svc::mode(c)))?;
    if wmode != "strava" {
        let done = with_conn(app, |c| Ok(crate::db::get_setting(c, "workout_backfill").as_deref() == Some("1")))?;
        if !done {
            let mut cursor: Option<String> = None;
            let mut complete = false;
            for _ in 0..500 {
                let Ok(page) = freddy::page_m(None, cursor.as_deref(), None, Some(WORKOUT_METRICS)).await else { break };
                out.pages += 1;
                if !page.rows.is_empty() {
                    out.rows += with_conn(app, |c| store::upsert_metrics(c, &page.rows))?;
                }
                match page.next {
                    Some(n) if page.seen > 0 => cursor = Some(n),
                    _ => {
                        complete = true;
                        break;
                    }
                }
            }
            if complete {
                let _ = with_conn(app, |c| crate::db::set_setting(c, "workout_backfill", "1"));
            }
        }
        let since = if with_conn(app, |c| Ok(crate::db::get_setting(c, "workout_raw_backfill").as_deref() == Some("1")))? {
            from.clone().unwrap_or_else(|| time::ymd(crate::store::now() - 14 * 86400, 0))
        } else {
            time::ymd(crate::store::now() - 90 * 86400, 0)
        };
        if let Ok(rows) = freddy::raw_rows("workout_raw", &since).await {
            if !rows.is_empty() {
                let _ = with_conn(app, |c| store::upsert_metrics(c, &rows));
            }
            let _ = with_conn(app, |c| crate::db::set_setting(c, "workout_raw_backfill", "1"));
        }
    }
    let _ = with_conn(app, |c| crate::workouts_svc::after_sync(c));
    // New metrics may have arrived: rebuild the view of the chosen sources.
    let _ = with_conn(app, |c| crate::sources::install_view(c));
    Ok(out)
}

/// Runs both syncs shortly after start and then every 20 minutes, while the app is unlocked.
pub fn start_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let unlocked = *app.state::<AppState>().unlocked.lock().unwrap() == Some(true);
            if unlocked {
                if strava::is_connected() {
                    let _ = strava_sync(&app, false).await;
                }
                if crate::secrets::has("freddy_api_key") {
                    let _ = freddy_sync(&app, false).await;
                }
                // Recompute so the morning score freezes and the evening one is saved even when
                // the app window is not looked at.
                let _ = with_conn(&app, |c| crate::engine_svc::summary(c).map(|_| ()));
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}
