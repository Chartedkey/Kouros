//! Workout sources (0.17): Strava, freddy (Apple Health, Garmin, Suunto…) or both; duplicates of the
//! same workout from different sources, rules per sport («спрашивать» / always one source / the most
//! detailed record); deleting workouts into «Удалённые» and back.
//!
//! activities.hidden: 0 counted, 1 deleted by the athlete, 2 another record of the same workout was
//! chosen, 3 duplicate waiting for a choice (the most detailed record is counted meanwhile),
//! 4 its source is switched off, 5 deleted for good (row kept so the sync never brings it back).
//! Every reader uses the TEMP VIEW `va` (hidden = 0).

use crate::{db, engine_svc, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn install_view(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("DROP VIEW IF EXISTS temp.va; CREATE TEMP VIEW va AS SELECT * FROM activities WHERE hidden = 0;").map_err(err)
}

/// Sport groups for rules and matching.
pub const GROUPS: [(&str, &str); 5] = [("run", "Бег"), ("walk", "Ходьба и походы"), ("strength", "Силовые и тренировки"), ("ride", "Велосипед"), ("other", "Остальное")];

pub fn group_of(sport: &str) -> &'static str {
    match sport {
        "run" | "trail_run" | "treadmill" | "run_virtual" => "run",
        "walk" | "hike" => "walk",
        "strength" | "workout" | "yoga" | "crossfit" => "strength",
        "ride" | "ride_virtual" | "ebike" => "ride",
        _ => "other",
    }
}

fn family(source: &str) -> String {
    if source.starts_with("freddy:") { source.to_string() } else { "strava".to_string() }
}

pub fn source_name(source: &str, device: Option<&str>) -> String {
    let base = if let Some(p) = source.strip_prefix("freddy:") {
        crate::sources::provider_name(p)
    } else if source.starts_with("strava") {
        "Strava".to_string()
    } else {
        "Файл".to_string()
    };
    match device.filter(|d| !d.is_empty()) {
        Some(d) => format!("{base} · {d}"),
        None => base,
    }
}

pub fn settings(conn: &Connection) -> Value {
    let v = db::get_setting(conn, "workout_sources").and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
    if v.is_object() { v } else { json!({"mode": "strava", "providers": [], "rules": {}}) }
}

pub fn mode(conn: &Connection) -> String {
    settings(conn)["mode"].as_str().unwrap_or("strava").to_string()
}

fn rule(cfg: &Value, group: &str) -> String {
    cfg["rules"][group].as_str().unwrap_or("ask").to_string()
}

fn provider_allowed(cfg: &Value, p: &str) -> bool {
    match cfg["providers"].as_array() {
        Some(a) if !a.is_empty() => a.iter().any(|x| x.as_str() == Some(p)),
        _ => true,
    }
}

// ---------- freddy workouts → activities ----------

fn apple_sport(t: &str) -> &'static str {
    let t = t.to_lowercase();
    if t.contains("run") {
        "run"
    } else if t.contains("hik") {
        "hike"
    } else if t.contains("walk") {
        "walk"
    } else if t.contains("cycl") || t.contains("bik") || t.contains("ride") {
        "ride"
    } else if t.contains("swim") {
        "swim"
    } else if t.contains("yoga") || t.contains("pilates") || t.contains("flexib") || t.contains("stretch") {
        "yoga"
    } else if t.contains("strength") || t.contains("core") || t.contains("weight") {
        "strength"
    } else if t.contains("hiit") || t.contains("interval") || t.contains("cross") || t.contains("cardio") || t.contains("functional") {
        "workout"
    } else {
        "other"
    }
}

fn sport_ru(s: &str) -> &'static str {
    match s {
        "run" => "Бег",
        "walk" => "Ходьба",
        "hike" => "Поход",
        "ride" => "Велосипед",
        "swim" => "Плавание",
        "yoga" => "Йога и растяжка",
        "strength" => "Силовая",
        "workout" => "Тренировка",
        _ => "Тренировка",
    }
}

/// Echo copies inside one source (the same watch under two names, apps that copy workouts into
/// Apple Health) — these are never asked about.
fn copier(device: &str) -> bool {
    let d = device.to_lowercase();
    d.contains("healthfit") || d.contains("health sync") || d.contains("sync") || d.contains("strava") || d.contains("фитнес")
}

/// Turns stored freddy workout rows (`workout_*` / `activity_*`) into activities. Raw heart rate
/// (`workout_raw`, points {t, v}) becomes the HR stream.
pub fn import_freddy(conn: &mut Connection) -> Result<usize, String> {
    let cfg = settings(conn);
    if cfg["mode"].as_str().unwrap_or("strava") == "strava" {
        return Ok(0);
    }
    struct W {
        provider: String,
        device: String,
        ts: String,
        f: BTreeMap<String, f64>,
        kind: Option<String>,
        raw: Option<String>,
    }
    let rows: Vec<(String, String, String, String, Option<f64>, Option<String>)> = {
        let mut st = conn
            .prepare(
                "SELECT provider, device, ts, metric, num, txt FROM health_metrics
                 WHERE (metric LIKE 'workout\\_%' ESCAPE '\\' OR metric LIKE 'activity\\_%' ESCAPE '\\') AND ts <> ''",
            )
            .map_err(err)?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))).map_err(err)?.flatten().collect();
        v
    };
    let mut ws: BTreeMap<(String, String, String), W> = BTreeMap::new();
    for (p, dev, ts, m, num, txt) in rows {
        if !provider_allowed(&cfg, &p) {
            continue;
        }
        let w = ws.entry((p.clone(), dev.clone(), ts.clone())).or_insert(W { provider: p, device: dev, ts, f: BTreeMap::new(), kind: None, raw: None });
        let key = m.trim_start_matches("workout_").trim_start_matches("activity_").to_string();
        if key.contains("type") || key == "sport" {
            w.kind = txt.clone().or(w.kind.clone());
        } else if key == "raw" || key.ends_with("hr_samples") || key.ends_with("heartrate_samples") {
            if txt.is_some() {
                w.raw = txt.clone();
            }
        } else if let Some(n) = num {
            w.f.insert(key, n);
        }
    }
    // Group echoes: same provider, start within 2 min, same sport.
    let mut list: Vec<W> = ws.into_values().filter(|w| w.kind.is_some() || w.f.contains_key("duration")).collect();
    list.sort_by(|a, b| a.ts.cmp(&b.ts));
    let start_of = |w: &W| crate::health_svc::iso(&w.ts).unwrap_or(0);
    let mut chosen: Vec<W> = Vec::new();
    for w in list {
        let sp = apple_sport(w.kind.as_deref().unwrap_or("other"));
        let st = start_of(&w);
        if let Some(c) = chosen.iter_mut().find(|c| c.provider == w.provider && (start_of(c) - st).abs() <= 120 && apple_sport(c.kind.as_deref().unwrap_or("other")) == sp) {
            // Keep the richer / original one.
            let score = |x: &W| (x.raw.is_some() as i32) * 4 + (!copier(&x.device)) as i32 * 2 + x.f.len() as i32 / 4;
            if score(&w) > score(c) {
                *c = w;
            }
            continue;
        }
        chosen.push(w);
    }
    let mut added = 0;
    let tx = conn.transaction().map_err(err)?;
    for w in &chosen {
        let st = start_of(w);
        if st == 0 {
            continue;
        }
        let sport = apple_sport(w.kind.as_deref().unwrap_or("other")).to_string();
        let id = format!("h{}-{}-{}", w.provider.to_lowercase().chars().take(6).collect::<String>(), st, sport);
        let exists: bool = tx.query_row("SELECT count(*) FROM activities WHERE id = ?1", params![id], |r| r.get::<_, i64>(0)).map(|n| n > 0).unwrap_or(false);
        let get = |k: &[&str]| k.iter().find_map(|x| w.f.get(*x).copied());
        let mut p = crate::parse::Parsed {
            sport: sport.clone(),
            sport_label: w.kind.clone(),
            name: Some(format!("{} · {}", sport_ru(&sport), if w.device.is_empty() { crate::sources::provider_name(&w.provider) } else { w.device.clone() })),
            start_utc: st,
            elapsed_s: get(&["duration", "elapsed_time", "elapsed_s", "duration_s"]),
            moving_s: get(&["moving_time", "moving_s"]),
            distance_m: get(&["distance", "distance_m"]),
            elev_gain_m: get(&["elevation_ascended", "elevation_gain", "total_elevation_gain"]),
            avg_hr: get(&["average_heart_rate", "avg_hr", "average_hr", "heart_rate_avg"]),
            max_hr: get(&["max_heart_rate", "max_hr"]),
            calories: get(&["active_energy", "calories", "kcal"]),
            device: Some(w.device.clone()).filter(|d| !d.is_empty()),
            ..Default::default()
        };
        if let Some(raw) = &w.raw {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                let pts = v["points"].as_array().or(v.as_array()).cloned().unwrap_or_default();
                for pt in pts {
                    let t = pt["t"].as_str().and_then(crate::health_svc::iso).or(pt["ts"].as_i64());
                    let hr = pt["v"].as_f64().or(pt["hr"].as_f64());
                    if let (Some(t), Some(hr)) = (t, hr) {
                        p.streams.push(t, crate::parse::Sample { hr: Some(hr), ..Default::default() });
                    }
                }
                p.fill_from_streams();
            }
        }
        if exists {
            // Streams may arrive later than the summary.
            if !p.streams.is_empty() {
                let had: i64 = tx.query_row("SELECT has_streams FROM activities WHERE id = ?1", params![id], |r| r.get(0)).unwrap_or(0);
                if had == 0 {
                    store::merge_activity(&tx, &id, None, None, &p, false)?;
                }
            }
            continue;
        }
        store::insert_activity(&tx, &id, None, &format!("freddy:{}", w.provider), None, &p)?;
        added += 1;
    }
    tx.commit().map_err(err)?;
    Ok(added)
}

// ---------- duplicates ----------

#[derive(Clone)]
struct Row {
    id: String,
    sport: String,
    start: i64,
    dur: f64,
    source: String,
    device: Option<String>,
    hidden: i64,
    group: Option<String>,
    state: Option<String>,
    streams: i64,
    gps: i64,
    avg_hr: Option<f64>,
    dist: Option<f64>,
}

fn detail(r: &Row) -> i64 {
    (r.streams == 1) as i64 * 3 + r.gps * 2 + r.avg_hr.is_some() as i64 + r.dist.is_some() as i64
}

pub fn detail_text(streams: i64, gps: i64, source: &str) -> &'static str {
    match (streams == 1, gps == 1, source.starts_with("freddy:")) {
        (true, true, _) => "пульс, темп и высота по секундам, трек GPS",
        (true, false, true) => "пульс по ходу (каждые ~15 с), без трека",
        (true, false, false) => "пульс и темп по секундам, без трека",
        _ => "только итоги — нагрузка посчитана приблизительно",
    }
}

fn rows(conn: &Connection) -> Vec<Row> {
    let Ok(mut st) = conn.prepare(
        "SELECT id, sport, start_utc, COALESCE(elapsed_s, moving_s, 0), source, device, hidden, dup_group, dup_state, has_streams, has_gps, avg_hr, distance_m
         FROM activities WHERE hidden IN (0, 2, 3) ORDER BY start_utc",
    ) else {
        return Vec::new();
    };
    let v = st
        .query_map([], |r| {
            Ok(Row {
                id: r.get(0)?, sport: r.get(1)?, start: r.get(2)?, dur: r.get(3)?, source: r.get(4)?, device: r.get(5)?, hidden: r.get(6)?,
                group: r.get(7)?, state: r.get(8)?, streams: r.get(9)?, gps: r.get(10)?, avg_hr: r.get(11)?, dist: r.get(12)?,
            })
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    v
}

fn same(a: &Row, b: &Row) -> bool {
    if group_of(&a.sport) != group_of(&b.sport) || family(&a.source) == family(&b.source) {
        return false;
    }
    if (a.start - b.start).abs() > 600 {
        return false;
    }
    let (a0, a1) = (a.start as f64, a.start as f64 + a.dur.max(60.0));
    let (b0, b1) = (b.start as f64, b.start as f64 + b.dur.max(60.0));
    let ov = (a1.min(b1) - a0.max(b0)).max(0.0);
    ov / (a1 - a0).max(b1 - b0) >= 0.6
}

fn set_row(conn: &Connection, id: &str, hidden: i64, group: Option<&str>, state: Option<&str>) -> Result<(), String> {
    conn.execute("UPDATE activities SET hidden = ?2, dup_group = ?3, dup_state = ?4 WHERE id = ?1", params![id, hidden, group, state]).map_err(err)?;
    Ok(())
}

/// Finds records of the same workout from different sources and applies the rule of its sport.
pub fn dedupe(conn: &Connection) -> Result<usize, String> {
    let cfg = settings(conn);
    let since_hist = cfg["enabledTs"].as_i64().unwrap_or(i64::MAX);
    let all = rows(conn);
    let mut new_pending = 0;
    let mut used = vec![false; all.len()];
    for i in 0..all.len() {
        if used[i] || all[i].state.as_deref() == Some("distinct") {
            continue;
        }
        let mut grp = vec![i];
        for j in i + 1..all.len() {
            if all[j].start - all[i].start > 600 {
                break;
            }
            if !used[j] && all[j].state.as_deref() != Some("distinct") && grp.iter().any(|k| same(&all[*k], &all[j])) {
                grp.push(j);
            }
        }
        if grp.len() < 2 {
            continue;
        }
        for k in &grp {
            used[*k] = true;
        }
        let members: Vec<&Row> = grp.iter().map(|k| &all[*k]).collect();
        let gid = members.iter().find_map(|m| m.group.clone()).unwrap_or_else(|| format!("g{}", members[0].start));
        let fresh: Vec<&&Row> = members.iter().filter(|m| m.group.is_none()).collect();
        let decided = members.iter().any(|m| m.state.as_deref() == Some("decided"));
        if decided {
            // A late record of a decided workout stays aside (it can be picked on the workout page).
            for m in fresh {
                set_row(conn, &m.id, 2, Some(&gid), Some("decided"))?;
            }
            continue;
        }
        let richest = members.iter().max_by_key(|m| detail(m)).unwrap();
        let history = members[0].start < since_hist - 86400;
        let r = rule(&cfg, group_of(&members[0].sport));
        let pick: Option<&&Row> = if history {
            members.iter().find(|m| family(&m.source) == "strava").or(Some(richest))
        } else {
            match r.as_str() {
                "best" => Some(richest),
                "strava" => members.iter().find(|m| family(&m.source) == "strava"),
                "ask" => None,
                p => members.iter().find(|m| m.source == format!("freddy:{p}")),
            }
        };
        match pick {
            Some(keep) => {
                for m in &members {
                    set_row(conn, &m.id, if m.id == keep.id { 0 } else { 2 }, Some(&gid), Some("decided"))?;
                }
            }
            None => {
                if members.iter().any(|m| m.state.is_none()) {
                    new_pending += 1;
                }
                for m in &members {
                    set_row(conn, &m.id, if m.id == richest.id { 0 } else { 3 }, Some(&gid), Some("pending"))?;
                }
            }
        }
    }
    Ok(new_pending)
}

/// Turns sources on and off by the mode (hidden 0 ↔ 4).
pub fn apply_mode(conn: &Connection) -> Result<(), String> {
    let cfg = settings(conn);
    let m = cfg["mode"].as_str().unwrap_or("strava");
    let strava_on = m != "freddy";
    let freddy_on = m != "strava";
    conn.execute(
        "UPDATE activities SET hidden = 4 WHERE hidden IN (0, 2, 3) AND ((source NOT LIKE 'freddy:%' AND ?1 = 0) OR (source LIKE 'freddy:%' AND ?2 = 0))",
        params![strava_on as i64, freddy_on as i64],
    )
    .map_err(err)?;
    conn.execute(
        "UPDATE activities SET hidden = 0, dup_group = NULL, dup_state = NULL WHERE hidden = 4 AND ((source NOT LIKE 'freddy:%' AND ?1 = 1) OR (source LIKE 'freddy:%' AND ?2 = 1))",
        params![strava_on as i64, freddy_on as i64],
    )
    .map_err(err)?;
    // Providers switched off inside freddy.
    if freddy_on {
        if let Some(a) = cfg["providers"].as_array().filter(|a| !a.is_empty()) {
            let allowed: Vec<String> = a.iter().filter_map(|x| x.as_str().map(|p| format!("freddy:{p}"))).collect();
            let rows: Vec<String> = {
                let mut st = conn.prepare("SELECT id, source FROM activities WHERE source LIKE 'freddy:%' AND hidden IN (0, 2, 3)").map_err(err)?;
                let v = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(err)?.flatten().filter(|(_, s)| !allowed.contains(s)).map(|x| x.0).collect();
                v
            };
            for id in rows {
                conn.execute("UPDATE activities SET hidden = 4 WHERE id = ?1", params![id]).map_err(err)?;
            }
        }
    }
    Ok(())
}

/// Runs after every sync.
pub fn after_sync(conn: &mut Connection) -> Result<(), String> {
    import_freddy(conn)?;
    apply_mode(conn)?;
    dedupe(conn)?;
    Ok(())
}

pub fn today_changed(conn: &Connection) {
    let today = engine_svc::today(&engine_svc::profile(conn));
    let _ = conn.execute("DELETE FROM readiness_log WHERE date = ?1", params![today]);
}

// ---------- deleted in Strava (1.3.2) ----------

fn setting_json(conn: &Connection, key: &str) -> Value {
    db::get_setting(conn, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!([]))
}

/// Workouts of the last week that Strava no longer lists. They are not removed silently: the
/// athlete is asked once (`strava_gone`), «Оставить» remembers the answer (`strava_gone_keep`).
pub fn mark_gone(conn: &Connection, missing: &[(String, i64, Option<String>, i64, String)]) -> usize {
    let keep: Vec<String> = serde_json::from_value(setting_json(conn, "strava_gone_keep")).unwrap_or_default();
    let mut list: Vec<Value> = setting_json(conn, "strava_gone").as_array().cloned().unwrap_or_default();
    let ids: Vec<&String> = missing.iter().map(|m| &m.0).collect();
    // Back in Strava (or answered) — no longer waiting.
    list.retain(|v| v["id"].as_str().map(|id| ids.iter().any(|x| x.as_str() == id)).unwrap_or(false));
    for (id, _, name, start, sport) in missing {
        if keep.contains(id) || list.iter().any(|v| v["id"].as_str() == Some(id.as_str())) {
            continue;
        }
        list.push(json!({"id": id, "name": name, "start": start, "sport": sport}));
    }
    let _ = db::set_setting(conn, "strava_gone", &Value::Array(list.clone()).to_string());
    list.len()
}

pub fn gone(conn: &Connection) -> Value {
    setting_json(conn, "strava_gone")
}

/// `delete` = also delete in Kouros (to «Удалённые», can be restored), else keep and don't ask again.
pub fn gone_decide(conn: &Connection, id: &str, delete: bool) -> Result<(), String> {
    let mut list: Vec<Value> = setting_json(conn, "strava_gone").as_array().cloned().unwrap_or_default();
    list.retain(|v| v["id"].as_str() != Some(id));
    db::set_setting(conn, "strava_gone", &Value::Array(list).to_string())?;
    if delete {
        return self::delete(conn, &[id.to_string()]);
    }
    let mut keep: Vec<String> = serde_json::from_value(setting_json(conn, "strava_gone_keep")).unwrap_or_default();
    keep.push(id.to_string());
    db::set_setting(conn, "strava_gone_keep", &serde_json::to_string(&keep).unwrap_or_default())
}

// ---------- settings and decisions ----------

pub fn save_settings(conn: &mut Connection, v: &Value) -> Result<(), String> {
    let mut cfg = settings(conn);
    for k in ["mode", "providers", "rules"] {
        if !v[k].is_null() {
            cfg[k] = v[k].clone();
        }
    }
    if cfg["mode"].as_str() != Some("strava") && cfg["enabledTs"].is_null() {
        cfg["enabledTs"] = json!(store::now());
    }
    db::set_setting(conn, "workout_sources", &cfg.to_string())?;
    after_sync(conn)?;
    today_changed(conn);
    Ok(())
}

pub fn pending(conn: &Connection) -> Vec<Value> {
    let tz = engine_svc::profile(conn).tz_offset_s;
    let Ok(mut st) = conn.prepare(
        "SELECT dup_group, id, sport, name, start_utc, COALESCE(moving_s, elapsed_s), distance_m, avg_hr, source, device, has_streams, has_gps, hidden
         FROM activities WHERE dup_state = 'pending' AND hidden IN (0, 3) ORDER BY start_utc DESC, id",
    ) else {
        return Vec::new();
    };
    let rows: Vec<(String, Value)> = st
        .query_map([], |r| {
            let src: String = r.get(8)?;
            let dev: Option<String> = r.get(9)?;
            let s: i64 = r.get(10)?;
            let g: i64 = r.get(11)?;
            Ok((r.get::<_, String>(0)?, json!({
                "id": r.get::<_, String>(1)?, "sport": r.get::<_, String>(2)?, "name": r.get::<_, Option<String>>(3)?, "start": r.get::<_, i64>(4)?,
                "min": (r.get::<_, Option<f64>>(5)?.unwrap_or(0.0) / 60.0).round(), "km": r.get::<_, Option<f64>>(6)?.map(|d| (d / 100.0).round() / 10.0),
                "avgHr": r.get::<_, Option<f64>>(7)?.map(|x| x.round()), "source": src, "sourceName": source_name(&src, dev.as_deref()),
                "detail": detail_text(s, g, &src), "counted": r.get::<_, i64>(12)? == 0,
            })))
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    let mut by: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (g, v) in rows {
        by.entry(g).or_default().push(v);
    }
    let mut out: Vec<Value> = by
        .into_iter()
        .map(|(g, m)| {
            let start = m[0]["start"].as_i64().unwrap_or(0);
            json!({"group": g, "date": crate::engine::day_of(start, tz), "sportGroup": group_of(m[0]["sport"].as_str().unwrap_or("")), "records": m})
        })
        .collect();
    out.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    out
}

/// Keep one record of a workout (`keep` = id) or all of them (`keep` = None, «это разные
/// тренировки»). `remember` = make it the rule for this sport.
pub fn decide(conn: &Connection, group: &str, keep: Option<&str>, remember: bool) -> Result<(), String> {
    let members: Vec<(String, String, String)> = {
        let mut st = conn.prepare("SELECT id, source, sport FROM activities WHERE dup_group = ?1").map_err(err)?;
        let v = st.query_map(params![group], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(err)?.flatten().collect();
        v
    };
    if members.is_empty() {
        return Err("Тренировка не найдена".into());
    }
    match keep {
        Some(k) => {
            for (id, _, _) in &members {
                set_row(conn, id, if id.as_str() == k { 0 } else { 2 }, Some(group), Some("decided"))?;
            }
            if remember {
                if let Some((_, src, sport)) = members.iter().find(|m| m.0 == k) {
                    let mut cfg = settings(conn);
                    let val = if src.starts_with("freddy:") { src.trim_start_matches("freddy:").to_string() } else { "strava".into() };
                    if !cfg["rules"].is_object() {
                        cfg["rules"] = json!({});
                    }
                    cfg["rules"][group_of(sport)] = json!(val);
                    db::set_setting(conn, "workout_sources", &cfg.to_string())?;
                }
            }
        }
        None => {
            for (id, _, _) in &members {
                set_row(conn, id, 0, None, Some("distinct"))?;
            }
        }
    }
    today_changed(conn);
    Ok(())
}

/// «Взять её вместо этой» on the workout page.
pub fn choose(conn: &Connection, id: &str) -> Result<(), String> {
    let g: Option<String> = conn.query_row("SELECT dup_group FROM activities WHERE id = ?1", params![id], |r| r.get(0)).map_err(err)?;
    let Some(g) = g else { return Err("У этой тренировки нет второй записи".into()) };
    decide(conn, &g, Some(id), false)
}

/// Other records of the same workout (for the workout page).
pub fn alternatives(conn: &Connection, id: &str) -> Vec<Value> {
    let g: Option<String> = conn.query_row("SELECT dup_group FROM activities WHERE id = ?1", params![id], |r| r.get(0)).ok().flatten();
    let Some(g) = g else { return Vec::new() };
    let Ok(mut st) = conn.prepare(
        "SELECT id, source, device, has_streams, has_gps, COALESCE(moving_s, elapsed_s), distance_m, avg_hr, hidden FROM activities WHERE dup_group = ?1 AND id <> ?2 AND hidden IN (0, 2, 3)",
    ) else {
        return Vec::new();
    };
    let v = st
        .query_map(params![g, id], |r| {
            let src: String = r.get(1)?;
            let dev: Option<String> = r.get(2)?;
            Ok(json!({"id": r.get::<_, String>(0)?, "sourceName": source_name(&src, dev.as_deref()), "detail": detail_text(r.get(3)?, r.get(4)?, &src),
                      "min": (r.get::<_, Option<f64>>(5)?.unwrap_or(0.0) / 60.0).round(), "km": r.get::<_, Option<f64>>(6)?.map(|d| (d / 100.0).round() / 10.0),
                      "avgHr": r.get::<_, Option<f64>>(7)?.map(|x| x.round()), "counted": r.get::<_, i64>(8)? == 0}))
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    v
}

// ---------- delete / restore ----------

pub fn delete(conn: &Connection, ids: &[String]) -> Result<(), String> {
    for id in ids {
        conn.execute("UPDATE activities SET hidden = 1, deleted_ts = ?2 WHERE id = ?1 AND hidden <> 5", params![id, store::now()]).map_err(err)?;
    }
    today_changed(conn);
    Ok(())
}

pub fn restore(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("UPDATE activities SET hidden = 0, deleted_ts = NULL, dup_group = NULL, dup_state = 'distinct' WHERE id = ?1 AND hidden = 1", params![id]).map_err(err)?;
    today_changed(conn);
    Ok(())
}

/// For good: the row stays as a marker (so Strava / freddy never bring it back), its data goes.
pub fn purge(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("UPDATE activities SET hidden = 5 WHERE id = ?1 AND hidden = 1", params![id]).map_err(err)?;
    for t in ["activity_streams", "strength_sets", "activity_metrics", "muscle_cache", "checkins"] {
        let _ = conn.execute(&format!("DELETE FROM {t} WHERE activity_id = ?1"), params![id]);
    }
    Ok(())
}

pub fn deleted(conn: &Connection) -> Vec<Value> {
    let Ok(mut st) = conn.prepare(
        "SELECT id, sport, name, start_utc, COALESCE(moving_s, elapsed_s), distance_m, source, device, deleted_ts FROM activities WHERE hidden = 1 ORDER BY deleted_ts DESC LIMIT 100",
    ) else {
        return Vec::new();
    };
    let v = st
        .query_map([], |r| {
            let src: String = r.get(6)?;
            let dev: Option<String> = r.get(7)?;
            Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?, "start": r.get::<_, i64>(3)?,
                      "min": (r.get::<_, Option<f64>>(4)?.unwrap_or(0.0) / 60.0).round(), "km": r.get::<_, Option<f64>>(5)?.map(|d| (d / 100.0).round() / 10.0),
                      "sourceName": source_name(&src, dev.as_deref()), "deletedTs": r.get::<_, Option<i64>>(8)?}))
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    v
}

pub fn state(conn: &Connection) -> Value {
    let cfg = settings(conn);
    let provs: Vec<Value> = crate::sources::catalog(conn)
        .iter()
        .filter(|c| c.metric.starts_with("workout_") || c.metric.starts_with("activity_"))
        .map(|c| c.provider.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .chain(std::iter::once("APPLE_HEALTH".to_string()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| json!({"provider": p, "name": crate::sources::provider_name(&p)}))
        .collect();
    let counts: Value = {
        let q = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
        json!({
            "strava": q("SELECT count(*) FROM activities WHERE source NOT LIKE 'freddy:%' AND hidden NOT IN (1, 5)"),
            "freddy": q("SELECT count(*) FROM activities WHERE source LIKE 'freddy:%' AND hidden NOT IN (1, 5)"),
            "pending": q("SELECT count(DISTINCT dup_group) FROM activities WHERE dup_state = 'pending'"),
            "deleted": q("SELECT count(*) FROM activities WHERE hidden = 1"),
        })
    };
    json!({
        "mode": cfg["mode"], "providers": cfg["providers"], "rules": cfg["rules"], "enabledTs": cfg["enabledTs"],
        "groups": GROUPS.iter().map(|(k, n)| json!({"key": k, "name": n, "rule": rule(&cfg, k)})).collect::<Vec<_>>(),
        "available": provs, "counts": counts,
    })
}
