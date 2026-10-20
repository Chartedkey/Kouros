//! Reading and writing workouts, health metrics and sync state.

use crate::parse::{time, Parsed};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn activity_id_for(strava_id: Option<i64>, p: &Parsed) -> String {
    match strava_id {
        Some(id) => format!("s{id}"),
        None => format!("f{}-{}", p.start_utc, p.sport),
    }
}

pub fn exists_strava(conn: &Connection, strava_id: i64) -> Option<(String, bool)> {
    conn.query_row(
        "SELECT id, has_streams FROM activities WHERE strava_id = ?1",
        params![strava_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? == 1)),
    )
    .optional()
    .ok()
    .flatten()
}

/// Finds a workout that starts within 2 minutes and lasts about as long.
pub fn find_by_time(conn: &Connection, start: i64, elapsed: Option<f64>) -> Option<(String, bool)> {
    let mut st = conn
        .prepare(
            "SELECT id, has_streams, elapsed_s FROM activities
             WHERE start_utc BETWEEN ?1 - 120 AND ?1 + 120 AND source NOT LIKE 'freddy:%' ORDER BY ABS(start_utc - ?1)",
        )
        .ok()?;
    let rows = st
        .query_map(params![start], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? == 1, r.get::<_, Option<f64>>(2)?))
        })
        .ok()?;
    for row in rows.flatten() {
        let (id, has, el) = row;
        let ok = match (elapsed, el) {
            (Some(a), Some(b)) => (a - b).abs() <= 120f64.max(0.1 * a.max(b)),
            _ => true,
        };
        if ok {
            return Some((id, has));
        }
    }
    None
}

/// Inserts a new workout with its streams and sets.
pub fn insert_activity(
    tx: &Transaction,
    id: &str,
    strava_id: Option<i64>,
    source: &str,
    file_path: Option<&str>,
    p: &Parsed,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO activities (id, strava_id, sport, sport_label, name, description, start_utc, elapsed_s, moving_s,
            distance_m, elev_gain_m, avg_hr, max_hr, avg_cadence, avg_power, calories, relative_effort, steps,
            total_sets, total_reps, weight_lifted_kg, gear, device, source, file_path, has_streams, has_gps)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
            ?23, ?24, ?25, ?26, ?27)",
        params![
            id, strava_id, p.sport, p.sport_label, p.name, p.description, p.start_utc, p.elapsed_s, p.moving_s,
            p.distance_m, p.elev_gain_m, p.avg_hr, p.max_hr, p.avg_cadence, p.avg_power, p.calories,
            p.relative_effort, p.steps, p.total_sets, p.total_reps, p.weight_lifted_kg, p.gear, p.device, source,
            file_path, !p.streams.is_empty() as i64, p.streams.has_gps() as i64
        ],
    )
    .map_err(err)?;
    write_details(tx, id, p)
}

fn write_details(tx: &Transaction, id: &str, p: &Parsed) -> Result<(), String> {
    if !p.streams.is_empty() {
        tx.execute(
            "INSERT INTO activity_streams (activity_id, data) VALUES (?1, ?2)
             ON CONFLICT(activity_id) DO UPDATE SET data = excluded.data",
            params![id, p.streams.to_json()],
        )
        .map_err(err)?;
    }
    if !p.sets.is_empty() {
        tx.execute("DELETE FROM strength_sets WHERE activity_id = ?1", params![id]).map_err(err)?;
        for (i, s) in p.sets.iter().enumerate() {
            tx.execute(
                "INSERT INTO strength_sets (activity_id, idx, start_utc, duration_s, reps, weight_kg, kind, category)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, i as i64, s.start_utc, s.duration_s, s.reps, s.weight_kg, s.kind, s.category],
            )
            .map_err(err)?;
        }
    }
    Ok(())
}

/// Adds what the existing workout lacks (streams, sets, empty fields) without
/// overwriting values it already has.
pub fn merge_activity(
    tx: &Transaction,
    id: &str,
    strava_id: Option<i64>,
    file_path: Option<&str>,
    p: &Parsed,
    had_streams: bool,
) -> Result<bool, String> {
    let changed = tx
        .execute(
            "UPDATE activities SET
                strava_id = COALESCE(strava_id, ?2),
                name = COALESCE(name, ?3), description = COALESCE(description, ?4),
                elapsed_s = COALESCE(elapsed_s, ?5), moving_s = COALESCE(moving_s, ?6),
                distance_m = COALESCE(distance_m, ?7), elev_gain_m = COALESCE(elev_gain_m, ?8),
                avg_hr = COALESCE(avg_hr, ?9), max_hr = COALESCE(max_hr, ?10),
                avg_cadence = COALESCE(avg_cadence, ?11), avg_power = COALESCE(avg_power, ?12),
                calories = COALESCE(calories, ?13), relative_effort = COALESCE(relative_effort, ?14),
                device = COALESCE(device, ?15), file_path = COALESCE(file_path, ?16),
                total_sets = COALESCE(total_sets, ?17), total_reps = COALESCE(total_reps, ?18),
                has_streams = CASE WHEN ?19 = 1 THEN 1 ELSE has_streams END, has_gps = MAX(has_gps, ?20),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![
                id, strava_id, p.name, p.description, p.elapsed_s, p.moving_s, p.distance_m, p.elev_gain_m,
                p.avg_hr, p.max_hr, p.avg_cadence, p.avg_power, p.calories, p.relative_effort, p.device, file_path,
                p.total_sets, p.total_reps, !p.streams.is_empty() as i64, p.streams.has_gps() as i64
            ],
        )
        .map_err(err)?
        > 0;
    if !had_streams && !p.streams.is_empty() {
        write_details(tx, id, p)?;
        return Ok(true);
    }
    if !p.sets.is_empty() {
        let n: i64 = tx
            .query_row("SELECT count(*) FROM strength_sets WHERE activity_id = ?1", params![id], |r| r.get(0))
            .unwrap_or(0);
        if n == 0 {
            write_details(tx, id, &Parsed { streams: Default::default(), ..p.clone() })?;
            return Ok(true);
        }
    }
    Ok(changed && !had_streams)
}

// ---------- reading ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRow {
    pub id: String,
    pub strava_id: Option<i64>,
    pub sport: String,
    pub sport_label: Option<String>,
    pub name: Option<String>,
    pub start_utc: i64,
    pub elapsed_s: Option<f64>,
    pub moving_s: Option<f64>,
    pub distance_m: Option<f64>,
    pub elev_gain_m: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub calories: Option<f64>,
    pub relative_effort: Option<f64>,
    pub total_sets: Option<i64>,
    pub total_reps: Option<i64>,
    pub device: Option<String>,
    pub source: String,
    pub has_streams: bool,
    pub has_gps: bool,
    pub file_path: Option<String>,
    pub rpe: Option<f64>,
    pub load: Option<f64>,
}

const ROW_COLS: &str = "id, strava_id, sport, sport_label, name, start_utc, elapsed_s, moving_s, distance_m, elev_gain_m,
    avg_hr, max_hr, calories, relative_effort, total_sets, total_reps, device, source, has_streams, has_gps, file_path, rpe,
    (SELECT m.load FROM activity_metrics m WHERE m.activity_id = activities.id)";

fn row(r: &rusqlite::Row) -> rusqlite::Result<ActivityRow> {
    Ok(ActivityRow {
        id: r.get(0)?,
        strava_id: r.get(1)?,
        sport: r.get(2)?,
        sport_label: r.get(3)?,
        name: r.get(4)?,
        start_utc: r.get(5)?,
        elapsed_s: r.get(6)?,
        moving_s: r.get(7)?,
        distance_m: r.get(8)?,
        elev_gain_m: r.get(9)?,
        avg_hr: r.get(10)?,
        max_hr: r.get(11)?,
        calories: r.get(12)?,
        relative_effort: r.get(13)?,
        total_sets: r.get(14)?,
        total_reps: r.get(15)?,
        device: r.get(16)?,
        source: r.get(17)?,
        has_streams: r.get::<_, i64>(18)? == 1,
        has_gps: r.get::<_, i64>(19)? != 0,
        file_path: r.get(20)?,
        rpe: r.get(21)?,
        load: r.get(22)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    pub total: i64,
    pub items: Vec<ActivityRow>,
}

pub struct ListFilter<'a> {
    pub sport: Option<&'a str>,
    /// Only workouts from the last N days.
    pub days: Option<i64>,
    /// Case-insensitive part of the name.
    pub query: Option<&'a str>,
    /// "date" | "distance" | "duration" | "load"
    pub sort: Option<&'a str>,
}

pub fn list_activities(conn: &Connection, f: &ListFilter, limit: i64, offset: i64) -> Result<ActivityPage, String> {
    let mut wh: Vec<String> = Vec::new();
    match f.sport {
        Some("bike") => wh.push("sport IN ('ride', 'ride_virtual')".into()),
        Some("other") => wh.push("sport NOT IN ('run', 'walk', 'hike', 'ride', 'ride_virtual', 'strength', 'workout')".into()),
        Some("walk") => wh.push("sport IN ('walk', 'hike')".into()),
        Some(s) if !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_') => wh.push(format!("sport = '{s}'")),
        _ => {}
    }
    if let Some(d) = f.days.filter(|d| *d > 0) {
        wh.push(format!("start_utc >= {}", now() - d * 86400));
    }
    let q = f.query.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    if q.is_some() {
        // SQLite's LIKE ignores case only for Latin letters, so Cyrillic is matched in three spellings.
        wh.push("(COALESCE(name, '') LIKE ?3 OR COALESCE(name, '') LIKE ?4 OR COALESCE(name, '') LIKE ?5)".into());
    }
    let filter = if wh.is_empty() { String::new() } else { format!("WHERE {}", wh.join(" AND ")) };
    let order = match f.sort {
        Some("distance") => "distance_m DESC NULLS LAST, start_utc DESC",
        Some("duration") => "COALESCE(moving_s, elapsed_s) DESC, start_utc DESC",
        Some("load") => "(SELECT m.load FROM activity_metrics m WHERE m.activity_id = activities.id) DESC NULLS LAST, start_utc DESC",
        _ => "start_utc DESC",
    };
    let typed = f.query.map(|q| q.trim().to_string()).unwrap_or_default();
    let like = q.as_ref().map(|q| format!("%{q}%")).unwrap_or_default();
    let cap = q
        .as_ref()
        .map(|q| {
            let mut c = q.chars();
            let first: String = c.next().map(|f| f.to_uppercase().collect()).unwrap_or_default();
            format!("%{first}{}%", c.as_str())
        })
        .unwrap_or_default();
    let typed = format!("%{typed}%");
    let total: i64 = if like.is_empty() {
        conn.query_row(&format!("SELECT count(*) FROM va AS activities {filter}"), [], |r| r.get(0))
    } else {
        let cf = filter.replace("?3", "?1").replace("?4", "?2").replace("?5", "?3");
        conn.query_row(&format!("SELECT count(*) FROM va AS activities {cf}"), params![like, cap, typed], |r| r.get(0))
    }
    .map_err(err)?;
    let mut st = conn
        .prepare(&format!("SELECT {ROW_COLS} FROM va AS activities {filter} ORDER BY {order} LIMIT ?1 OFFSET ?2"))
        .map_err(err)?;
    let items = if like.is_empty() {
        st.query_map(params![limit, offset], row).map_err(err)?.collect::<Result<Vec<_>, _>>()
    } else {
        st.query_map(params![limit, offset, like, cap, typed], row).map_err(err)?.collect::<Result<Vec<_>, _>>()
    }
    .map_err(err)?;
    Ok(ActivityPage { total, items })
}

pub fn activity_row(conn: &Connection, id: &str) -> Result<ActivityRow, String> {
    conn.query_row(&format!("SELECT {ROW_COLS} FROM activities WHERE id = ?1"), params![id], row)
        .map_err(|_| "Тренировка не найдена".to_string())
}

pub fn activities_between(conn: &Connection, from: i64, to: i64) -> Result<Vec<ActivityRow>, String> {
    let mut st = conn
        .prepare(&format!(
            "SELECT {ROW_COLS} FROM va AS activities WHERE start_utc BETWEEN ?1 AND ?2 ORDER BY start_utc"
        ))
        .map_err(err)?;
    let out = st
        .query_map(params![from, to], row)
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err);
    out
}

pub fn streams_json(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row("SELECT data FROM activity_streams WHERE activity_id = ?1", params![id], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SportCount {
    pub sport: String,
    pub count: i64,
    pub distance_m: f64,
    pub seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityStats {
    pub count: i64,
    pub first_utc: Option<i64>,
    pub last_utc: Option<i64>,
    pub with_streams: i64,
    pub by_sport: Vec<SportCount>,
}

pub fn activity_stats(conn: &Connection) -> Result<ActivityStats, String> {
    let (count, first_utc, last_utc, with_streams) = conn
        .query_row(
            "SELECT count(*), min(start_utc), max(start_utc), COALESCE(sum(has_streams = 1), 0) FROM va",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(err)?;
    let mut st = conn
        .prepare(
            "SELECT sport, count(*), COALESCE(sum(distance_m), 0), COALESCE(sum(COALESCE(moving_s, elapsed_s)), 0)
             FROM va GROUP BY sport ORDER BY count(*) DESC",
        )
        .map_err(err)?;
    let by_sport = st
        .query_map([], |r| {
            Ok(SportCount { sport: r.get(0)?, count: r.get(1)?, distance_m: r.get(2)?, seconds: r.get(3)? })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(ActivityStats { count, first_utc, last_utc, with_streams, by_sport })
}

/// Saves streams fetched later (Strava API); an empty result marks the workout as "no streams".
pub fn set_streams(conn: &Connection, id: &str, st: &crate::parse::Streams) -> Result<(), String> {
    if st.is_empty() {
        conn.execute("UPDATE activities SET has_streams = 2 WHERE id = ?1", params![id]).map_err(err)?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO activity_streams (activity_id, data) VALUES (?1, ?2)
         ON CONFLICT(activity_id) DO UPDATE SET data = excluded.data",
        params![id, st.to_json()],
    )
    .map_err(err)?;
    conn.execute(
        "UPDATE activities SET has_streams = 1, has_gps = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![id, st.has_gps() as i64],
    )
    .map_err(err)?;
    Ok(())
}

pub fn strava_missing_streams(conn: &Connection, limit: usize) -> Vec<(String, i64)> {
    let Ok(mut st) = conn.prepare(
        "SELECT id, strava_id FROM activities WHERE strava_id IS NOT NULL AND has_streams = 0 AND hidden <> 5
         ORDER BY start_utc DESC LIMIT ?1",
    ) else {
        return Vec::new();
    };
    let out = st
        .query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    out
}

pub fn latest_strava_start(conn: &Connection) -> Option<i64> {
    conn.query_row("SELECT max(start_utc) FROM activities WHERE strava_id IS NOT NULL", [], |r| r.get(0))
        .ok()
        .flatten()
}

pub fn count_activities(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM activities", [], |r| r.get(0)).unwrap_or(0)
}

// ---------- import log ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLog {
    pub at: String,
    pub title: String,
    pub kind: String,
    pub found: i64,
    pub added: i64,
    pub merged: i64,
    pub skipped: i64,
    pub errors: i64,
}

pub fn add_import_log(conn: &Connection, l: &ImportLog) {
    let _ = conn.execute(
        "INSERT INTO import_log (title, kind, found, added, merged, skipped, errors) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![l.title, l.kind, l.found, l.added, l.merged, l.skipped, l.errors],
    );
}

pub fn import_history(conn: &Connection) -> Result<Vec<ImportLog>, String> {
    let mut st = conn
        .prepare("SELECT at, title, kind, found, added, merged, skipped, errors FROM import_log ORDER BY id DESC LIMIT 20")
        .map_err(err)?;
    let out = st
        .query_map([], |r| {
            Ok(ImportLog {
                at: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
                found: r.get(3)?,
                added: r.get(4)?,
                merged: r.get(5)?,
                skipped: r.get(6)?,
                errors: r.get(7)?,
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err);
    out
}

// ---------- sync state ----------

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub cursor: Option<String>,
    pub last_ok: Option<String>,
    pub last_error: Option<String>,
    pub extra: Option<String>,
}

pub fn sync_state(conn: &Connection, source: &str) -> SyncState {
    conn.query_row(
        "SELECT cursor, last_ok, last_error, extra FROM sync_state WHERE source = ?1",
        params![source],
        |r| Ok(SyncState { cursor: r.get(0)?, last_ok: r.get(1)?, last_error: r.get(2)?, extra: r.get(3)? }),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub fn set_sync_ok(conn: &Connection, source: &str, cursor: Option<&str>, extra: Option<&str>) {
    let _ = conn.execute(
        "INSERT INTO sync_state (source, cursor, last_ok, last_error, extra) VALUES (?1, ?2, ?3, NULL, ?4)
         ON CONFLICT(source) DO UPDATE SET cursor = COALESCE(excluded.cursor, cursor), last_ok = excluded.last_ok,
            last_error = NULL, extra = COALESCE(excluded.extra, extra)",
        params![source, cursor, time::iso(now()), extra],
    );
    let _ = conn.execute(
        "UPDATE sources SET connected = 1, last_sync = ?2, last_error = NULL WHERE id = ?1",
        params![source, time::iso(now())],
    );
}

pub fn set_sync_error(conn: &Connection, source: &str, e: &str) {
    let _ = conn.execute(
        "INSERT INTO sync_state (source, last_error) VALUES (?1, ?2)
         ON CONFLICT(source) DO UPDATE SET last_error = excluded.last_error",
        params![source, e],
    );
    let _ = conn.execute("UPDATE sources SET last_error = ?2 WHERE id = ?1", params![source, e]);
}

pub fn set_source_connected(conn: &Connection, source: &str, connected: bool) {
    let _ = conn.execute("UPDATE sources SET connected = ?2 WHERE id = ?1", params![source, connected as i64]);
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------- health metrics ----------

pub struct MetricRow {
    pub metric: String,
    pub date: String,
    pub ts: String,
    pub provider: String,
    pub device: String,
    pub source_id: String,
    pub source: Option<String>,
    pub num: Option<f64>,
    pub txt: Option<String>,
    pub unit: Option<String>,
}

pub fn upsert_metrics(conn: &mut Connection, rows: &[MetricRow]) -> Result<usize, String> {
    let tx = conn.transaction().map_err(err)?;
    {
        let mut st = tx
            .prepare(
                "INSERT INTO health_metrics (metric, date, ts, provider, device, source_id, source, num, txt, unit)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(metric, date, ts, provider, device, source_id)
                 DO UPDATE SET num = excluded.num, txt = COALESCE(excluded.txt, health_metrics.txt), unit = excluded.unit, source = excluded.source",
            )
            .map_err(err)?;
        for r in rows {
            st.execute(params![r.metric, r.date, r.ts, r.provider, r.device, r.source_id, r.source, r.num, r.txt, r.unit])
                .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(rows.len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricLatest {
    pub metric: String,
    pub date: String,
    pub num: Option<f64>,
    pub unit: Option<String>,
    pub device: String,
    pub count: i64,
}

/// Latest value per metric for the given names (daily totals and merged values preferred by the caller).
pub fn latest_metrics(conn: &Connection, names: &[&str]) -> Result<Vec<MetricLatest>, String> {
    let mut out = Vec::new();
    for n in names {
        let r = conn
            .query_row(
                "SELECT metric, date, num, unit, device, (SELECT count(*) FROM health_metrics WHERE metric = ?1)
                 FROM health_metrics WHERE metric = ?1 AND num IS NOT NULL ORDER BY date DESC, ts DESC LIMIT 1",
                params![n],
                |r| {
                    Ok(MetricLatest {
                        metric: r.get(0)?,
                        date: r.get(1)?,
                        num: r.get(2)?,
                        unit: r.get(3)?,
                        device: r.get(4)?,
                        count: r.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(err)?;
        if let Some(r) = r {
            out.push(r);
        }
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStats {
    pub rows: i64,
    pub metrics: i64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

pub fn health_stats(conn: &Connection) -> Result<HealthStats, String> {
    conn.query_row(
        "SELECT count(*), count(DISTINCT metric), min(date), max(date) FROM health_metrics",
        [],
        |r| Ok(HealthStats { rows: r.get(0)?, metrics: r.get(1)?, first_date: r.get(2)?, last_date: r.get(3)? }),
    )
    .map_err(err)
}
