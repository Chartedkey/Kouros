//! «Тренер», part 3 (0.16): the post-workout check-in (RPE, feeling, where it hurt), the pain
//! journal on the body figure, the return-to-run steps, «Что лучше сегодня» from the athlete's own
//! recovery tools, and progress to the goal with control tests.

use crate::engine::{add_days, day_num, day_of, training::riegel};
use crate::{coach_svc, db, engine_svc, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Pain areas: key, name, affects running, affects upper-body / back strength.
pub const AREAS: [(&str, &str, bool, bool); 16] = [
    ("foot", "Стопа", true, false),
    ("ankles", "Ахилл и голеностоп", true, false),
    ("shin", "Голень", true, false),
    ("calves", "Икры", true, false),
    ("knees", "Колено", true, false),
    ("quads", "Передняя поверхность бедра", true, false),
    ("hams", "Задняя поверхность бедра", true, false),
    ("hips", "Тазобедренный сустав", true, false),
    ("glutes", "Ягодицы", true, false),
    ("back", "Спина", false, true),
    ("core", "Живот и кор", false, true),
    ("chest", "Грудь", false, true),
    ("shoulders", "Плечо", false, true),
    ("arms", "Рука", false, true),
    ("neck", "Шея", false, true),
    ("other", "Другое", false, false),
];

pub fn area_name(k: &str) -> &'static str {
    AREAS.iter().find(|a| a.0 == k).map(|a| a.1).unwrap_or("Другое")
}

fn side_word(s: &str) -> &'static str {
    match s {
        "L" => " (левая сторона)",
        "R" => " (правая сторона)",
        _ => "",
    }
}

fn today(conn: &Connection) -> (String, i64) {
    let p = engine_svc::profile(conn);
    (engine_svc::today(&p), p.tz_offset_s)
}

// ---------- check-in ----------

/// Workouts of the last 36 hours without a check-in (newest first), for the «Как прошла тренировка?» card.
pub fn pending(conn: &Connection) -> Vec<Value> {
    let since = store::now() - 36 * 3600;
    let Ok(mut st) = conn.prepare(
        "SELECT a.id, a.sport, a.name, a.start_utc, COALESCE(a.moving_s, a.elapsed_s), a.distance_m, a.rpe, a.shoe_id FROM va a
         WHERE a.start_utc >= ?1 AND NOT EXISTS (SELECT 1 FROM checkins c WHERE c.activity_id = a.id) ORDER BY a.start_utc DESC LIMIT 3",
    ) else {
        return Vec::new();
    };
    let v: Vec<Value> = st
        .query_map(params![since], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "sport": r.get::<_, String>(1)?, "name": r.get::<_, Option<String>>(2)?,
                      "start": r.get::<_, i64>(3)?, "min": (r.get::<_, Option<f64>>(4)?.unwrap_or(0.0) / 60.0).round(),
                      "km": r.get::<_, Option<f64>>(5)?.map(|d| (d / 100.0).round() / 10.0), "rpe": r.get::<_, Option<i64>>(6)?,
                      "shoeId": r.get::<_, Option<String>>(7)?}))
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    // Short easy walks don't need a question.
    v.into_iter()
        .filter(|a| !(a["sport"] == "walk" && a["min"].as_f64().unwrap_or(0.0) < 30.0))
        .map(|mut a| {
            // 0.23.1: «В каких кроссовках?» inside the check-in (runs; walks when a pair is worn for walking).
            a["askShoe"] = json!(crate::activity_svc::ask_shoe(conn, a["sport"].as_str().unwrap_or("")));
            a
        })
        .collect()
}

/// Saves the check-in: RPE goes to the workout (it sets the load of strength-type sessions), pains
/// go to the journal. `skip` = «не сейчас» (no more questions about this workout).
pub fn save_checkin(conn: &Connection, id: &str, v: &Value) -> Result<(), String> {
    let (td, tz) = today(conn);
    let start: i64 = conn.query_row("SELECT start_utc FROM activities WHERE id = ?1", params![id], |r| r.get(0)).map_err(err)?;
    let date = day_of(start, tz);
    let rpe = v["rpe"].as_i64().filter(|x| (1..=10).contains(x));
    let feel = v["feel"].as_i64().filter(|x| (1..=5).contains(x));
    let pains: Vec<(String, String, i64)> = v["pain"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| Some((p["area"].as_str()?.to_string(), p["side"].as_str().unwrap_or("").to_string(), p["level"].as_i64()?.clamp(0, 10)))).collect())
        .unwrap_or_default();
    conn.execute(
        "INSERT INTO checkins (activity_id, ts, rpe, feel, pain, note, skipped) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(activity_id) DO UPDATE SET ts = excluded.ts, rpe = excluded.rpe, feel = excluded.feel, pain = excluded.pain, note = excluded.note, skipped = excluded.skipped",
        params![id, store::now(), rpe, feel, json!(pains.iter().map(|(a, s, l)| json!({"area": a, "side": s, "level": l})).collect::<Vec<_>>()).to_string(),
                v["note"].as_str().filter(|s| !s.trim().is_empty()), v["skip"].as_bool().unwrap_or(false) as i64],
    )
    .map_err(err)?;
    if let Some(r) = rpe {
        conn.execute("UPDATE activities SET rpe = ?2 WHERE id = ?1", params![id, r]).map_err(err)?;
    }
    if let Some(sh) = v["shoe"].as_str().filter(|x| !x.is_empty()) {
        crate::activity_svc::set_shoe(conn, id, Some(sh.to_string()))?;
    }
    for (a, s, l) in &pains {
        log_pain(conn, &date, a, s, *l, "checkin", Some(id), None)?;
    }
    // A return-to-run session: remember the pain during it.
    let mut rt = get(conn, "return_to_run");
    if rt["active"].as_bool().unwrap_or(false) && date >= rt["started"].as_str().unwrap_or("").to_string() {
        let sport: String = conn.query_row("SELECT sport FROM activities WHERE id = ?1", params![id], |r| r.get(0)).unwrap_or_default();
        if sport.contains("run") || sport == "walk" {
            let area = rt["area"].as_str().unwrap_or("").to_string();
            let during = pains.iter().filter(|p| p.0 == area).map(|p| p.2).max().unwrap_or(0);
            rt["session"] = json!({"date": date, "stage": rt["stage"], "during": during});
            set(conn, "return_to_run", &rt)?;
        }
    }
    let _ = td;
    Ok(())
}

// ---------- pain journal ----------

fn get(conn: &Connection, key: &str) -> Value {
    db::get_setting(conn, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

fn set(conn: &Connection, key: &str, v: &Value) -> Result<(), String> {
    db::set_setting(conn, key, &v.to_string())
}

pub fn log_pain(conn: &Connection, date: &str, area: &str, side: &str, level: i64, ctx: &str, act: Option<&str>, note: Option<&str>) -> Result<(), String> {
    // One value per area, side, day and context: the latest replaces the earlier one.
    conn.execute("DELETE FROM pain_log WHERE date = ?1 AND area = ?2 AND side = ?3 AND context = ?4 AND (activity_id IS ?5)", params![date, area, side, ctx, act])
        .map_err(err)?;
    conn.execute(
        "INSERT INTO pain_log (date, ts, area, side, level, context, activity_id, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![date, store::now(), area, side, level.clamp(0, 10), ctx, act, note],
    )
    .map_err(err)?;
    Ok(())
}

/// A pain marked by hand (or the morning «как сейчас?» answer).
pub fn pain_mark(conn: &Connection, v: &Value) -> Result<(), String> {
    let (td, _) = today(conn);
    let area = v["area"].as_str().ok_or("Не выбрано место")?;
    let side = v["side"].as_str().unwrap_or("");
    let level = v["level"].as_i64().unwrap_or(0);
    let ctx = v["context"].as_str().unwrap_or("manual");
    let date = v["date"].as_str().unwrap_or(&td).to_string();
    log_pain(conn, &date, area, side, level, ctx, None, v["note"].as_str())?;
    if ctx == "morning" {
        return_eval(conn, &date, area, level)?;
    }
    Ok(())
}

pub fn pain_delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM pain_log WHERE id = ?1", params![id]).map_err(err)?;
    Ok(())
}

/// Per area and side: last level, its date, the trend over the last 14 days, and whether it is
/// still "active" (≥3/10 within the last 3 days).
pub fn pains(conn: &Connection) -> Vec<Value> {
    let (td, _) = today(conn);
    let from = add_days(&td, -60);
    let rows: Vec<(i64, String, String, String, i64, String, Option<String>)> = conn
        .prepare("SELECT id, date, area, side, level, context, note FROM pain_log WHERE date >= ?1 ORDER BY date, ts")
        .ok()
        .map(|mut st| {
            let v: Vec<_> = st
                .query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
                .map(|r| r.flatten().collect())
                .unwrap_or_default();
            v
        })
        .unwrap_or_default();
    let mut keys: Vec<(String, String)> = rows.iter().map(|r| (r.2.clone(), r.3.clone())).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for (a, s) in keys {
        let mine: Vec<&(i64, String, String, String, i64, String, Option<String>)> = rows.iter().filter(|r| r.2 == a && r.3 == s).collect();
        // Daily maximum.
        let mut by: std::collections::BTreeMap<String, i64> = Default::default();
        for r in &mine {
            let e = by.entry(r.1.clone()).or_insert(0);
            *e = (*e).max(r.4);
        }
        let last = mine.last().unwrap();
        let recent: Vec<i64> = by.range(add_days(&td, -2)..).map(|(_, v)| *v).collect();
        let active = recent.iter().any(|v| *v >= 3);
        let w1: Vec<f64> = by.range(add_days(&td, -6)..).map(|(_, v)| *v as f64).collect();
        let w0: Vec<f64> = by.range(add_days(&td, -13)..add_days(&td, -6)).map(|(_, v)| *v as f64).collect();
        let trend = match (crate::engine::mean(&w0), crate::engine::mean(&w1)) {
            (Some(a), Some(b)) if b < a - 0.7 => "better",
            (Some(a), Some(b)) if b > a + 0.7 => "worse",
            (Some(_), Some(_)) => "same",
            _ => "new",
        };
        let days_with = by.range(add_days(&td, -13)..).filter(|(_, v)| **v >= 3).count();
        out.push(json!({
            "area": a, "side": s, "name": format!("{}{}", area_name(&a), side_word(&s)), "level": last.4, "date": last.1,
            "active": active, "trend": trend, "days14": days_with,
            "series": by.iter().map(|(d, v)| json!([d, v])).collect::<Vec<_>>(),
            "entries": mine.iter().rev().take(12).map(|r| json!({"id": r.0, "date": r.1, "level": r.4, "context": r.5, "note": r.6})).collect::<Vec<_>>(),
        }));
    }
    out.sort_by(|a, b| b["active"].as_bool().cmp(&a["active"].as_bool()).then(b["level"].as_i64().cmp(&a["level"].as_i64())));
    out
}

/// Active pains as (area, level, runs affected, upper affected) — for the plan check.
pub fn active(conn: &Connection) -> Vec<(String, String, i64, bool, bool)> {
    pains(conn)
        .iter()
        .filter(|p| p["active"].as_bool().unwrap_or(false))
        .filter_map(|p| {
            let a = p["area"].as_str()?.to_string();
            let ar = AREAS.iter().find(|x| x.0 == a)?;
            Some((a.clone(), p["name"].as_str().unwrap_or("").to_string(), p["level"].as_i64().unwrap_or(0), ar.2, ar.3))
        })
        .collect()
}

// ---------- return to run ----------

/// Steps: plan item kind, minutes, text.
pub const STEPS: [(&str, f64, &str); 6] = [
    ("walk", 30.0, "Бодрая ходьба 30 минут"),
    ("walkrun", 20.0, "5 × (1 мин бег / 2 мин ходьба)"),
    ("walkrun", 25.0, "6 × (2 мин бег / 1 мин ходьба)"),
    ("walkrun", 25.0, "5 × (4 мин бег / 1 мин ходьба)"),
    ("easy", 20.0, "Лёгкий бег 20 минут без остановок"),
    ("easy", 30.0, "Лёгкий бег 30 минут"),
];

pub fn return_state(conn: &Connection) -> Value {
    let mut v = get(conn, "return_to_run");
    if !v.is_object() {
        v = json!({"active": false});
    }
    let st = v["stage"].as_u64().unwrap_or(0) as usize;
    v["steps"] = json!(STEPS.iter().map(|s| s.2).collect::<Vec<_>>());
    v["stageText"] = json!(STEPS.get(st).map(|s| s.2).unwrap_or(""));
    v["areaName"] = json!(area_name(v["area"].as_str().unwrap_or("other")));
    v
}

/// The current step as a plan item, if the return is active.
pub fn return_item(conn: &Connection) -> Option<(String, f64, usize)> {
    let v = get(conn, "return_to_run");
    if !v["active"].as_bool().unwrap_or(false) {
        return None;
    }
    let st = (v["stage"].as_u64().unwrap_or(0) as usize).min(STEPS.len() - 1);
    Some((STEPS[st].0.to_string(), STEPS[st].1, st))
}

pub fn return_start(conn: &Connection, area: &str, stage: usize) -> Result<(), String> {
    let (td, _) = today(conn);
    let v = json!({"active": true, "area": area, "stage": stage.min(STEPS.len() - 1), "started": td, "history": [{"date": td, "stage": stage, "why": "начало"}]});
    set(conn, "return_to_run", &v)?;
    crate::plan_svc::refresh_future(conn)
}

pub fn return_stop(conn: &Connection) -> Result<(), String> {
    let mut v = get(conn, "return_to_run");
    v["active"] = json!(false);
    set(conn, "return_to_run", &v)?;
    crate::plan_svc::refresh_future(conn)
}

/// The morning after a session: ≤3/10 during and now → next step; 4–5 → repeat; ≥6 → a step back.
fn return_eval(conn: &Connection, date: &str, area: &str, now_level: i64) -> Result<(), String> {
    let mut v = get(conn, "return_to_run");
    if !v["active"].as_bool().unwrap_or(false) || v["area"].as_str() != Some(area) {
        return Ok(());
    }
    let Some(sess) = v["session"].as_object().cloned() else { return Ok(()) };
    let sd = sess.get("date").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if sd.as_str() >= date || v["evaluated"].as_str() == Some(sd.as_str()) {
        return Ok(());
    }
    let during = sess.get("during").and_then(|x| x.as_i64()).unwrap_or(0);
    let worst = during.max(now_level);
    let st = v["stage"].as_i64().unwrap_or(0);
    let (next, why) = if worst <= 3 {
        (st + 1, format!("боль {worst}/10 — следующий шаг"))
    } else if worst <= 5 {
        (st, format!("боль {worst}/10 — повторяем шаг"))
    } else {
        ((st - 1).max(0), format!("боль {worst}/10 — шаг назад"))
    };
    v["evaluated"] = json!(sd);
    if next >= STEPS.len() as i64 {
        v["active"] = json!(false);
        v["finished"] = json!(date);
    }
    v["stage"] = json!(next.min(STEPS.len() as i64 - 1));
    if let Some(h) = v["history"].as_array_mut() {
        h.push(json!({"date": date, "stage": next, "why": why}));
    }
    set(conn, "return_to_run", &v)?;
    crate::plan_svc::refresh_future(conn)
}

// ---------- recovery tools ----------

/// «Что лучше сегодня» from the tools chosen in the onboarding.
pub fn tools(conn: &Connection, s: &Value) -> Value {
    let cp = coach_svc::profile(conn);
    let have: Vec<String> = cp["tools"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let (td, tz) = today(conn);
    let now = store::now();
    let hour = ((now + tz).rem_euclid(86400)) / 3600;
    let today_load = s["day"]["todayLoad"].as_f64().unwrap_or(0.0);
    let usual = s["day"]["usualLoad"].as_f64().unwrap_or(0.0).max(1.0);
    let acts = s["day"]["todayActivities"].as_array().cloned().unwrap_or_default();
    let ran = acts.iter().any(|a| a["sport"].as_str().map(|x| x.contains("run")).unwrap_or(false));
    let gym_h: Option<f64> = acts
        .iter()
        .filter(|a| matches!(a["sport"].as_str(), Some("strength") | Some("workout")))
        .filter_map(|a| a["startUtc"].as_i64().or(a["start"].as_i64()))
        .map(|t| (now - t) as f64 / 3600.0)
        .fold(None, |m: Option<f64>, x| Some(m.map(|y| y.min(x)).unwrap_or(x)));
    let legs: Vec<String> = s["muscleMap"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|g| g["state"] == "loaded" || g["state"] == "over")
                .filter(|g| matches!(g["key"].as_str(), Some("calves" | "quads" | "hams" | "glutes" | "hips" | "knees" | "ankles")))
                .filter_map(|g| g["name"].as_str().map(|x| x.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    let stress_hi = match (s["stress"]["today"]["score"].as_f64(), s["stress"]["today"]["norm"].as_f64()) {
        (Some(a), Some(b)) => a > b + 8.0,
        _ => false,
    };
    let ill = s["signals"].as_array().map(|a| a.iter().any(|g| matches!(g["code"].as_str(), Some("rhr_up" | "temp_up" | "low_rhr_low_hrv")))).unwrap_or(false);
    let heavy = today_load > usual * 1.3;
    let pains = active(conn);
    let tomorrow = crate::plan_svc::day_items_text(conn, &add_days(&td, 1));
    let tomorrow_hard = crate::plan_svc::day_hard(conn, &add_days(&td, 1));
    let mut good: Vec<(i64, String, String)> = Vec::new();
    let mut not: Vec<(String, String)> = Vec::new();
    for t in &have {
        let n = t.as_str();
        match n {
            "Роллер" => {
                if ran || !legs.is_empty() {
                    let g = if legs.is_empty() { "икры, бёдра и ягодицы".to_string() } else { legs.join(", ") };
                    good.push((80, n.into(), format!("10 минут: {g} — по 1–2 минуты на группу, медленно.")));
                } else {
                    good.push((20, n.into(), "5 минут на икры и бёдра — профилактика.".into()));
                }
            }
            "Перкуссионный массажёр" => {
                if let Some(p) = pains.iter().find(|p| p.2 >= 5) {
                    not.push((n.into(), format!("не по больному месту ({}, {}/10) — там может усилить воспаление", p.1.to_lowercase(), p.2)));
                } else if !legs.is_empty() || gym_h.is_some() {
                    good.push((60, n.into(), format!("1–2 минуты на группу: {}.", if legs.is_empty() { "мышцы, которые работали в зале".to_string() } else { legs.join(", ") })));
                }
            }
            "Компрессионная одежда" => {
                if heavy || (ran && today_load > usual) {
                    good.push((65, n.into(), "Надень на 2–3 часа после тренировки — ногам будет легче завтра.".into()));
                }
            }
            "Лимфодренажный массажёр для ног" => {
                if heavy || legs.len() >= 2 || tomorrow_hard {
                    good.push((70, n.into(), format!("20–30 минут вечером{}.", if tomorrow_hard { " — завтра тяжёлая тренировка" } else { "" })));
                }
            }
            "Сауна" => {
                if ill {
                    not.push((n.into(), "есть признаки, похожие на начало болезни (пульс или температура выше нормы) — сауна сейчас лишняя".into()));
                } else if heavy && hour < 20 {
                    not.push((n.into(), "после тяжёлой тренировки сначала вода и еда; сауна — не раньше чем через 2 часа".into()));
                } else if stress_hi || today_load <= usual {
                    good.push((if stress_hi { 75 } else { 45 }, n.into(), "2–3 захода по 10–15 минут, между ними прохладный душ; пей воду. Не позже чем за 2 часа до сна.".into()));
                }
            }
            "Горячая ванна" => {
                if ill {
                    not.push((n.into(), "при признаках болезни лучше без горячей ванны".into()));
                } else if hour >= 18 || stress_hi {
                    good.push((if stress_hi { 72 } else { 50 }, n.into(), "15 минут при 38–40° за 1–2 часа до сна — помогает уснуть.".into()));
                }
            }
            "Холодный душ или ванна" => {
                if gym_h.map(|h| h < 6.0).unwrap_or(false) {
                    not.push((n.into(), "в первые часы после силовой холод тормозит рост силы и мышц".into()));
                } else if hour >= 20 {
                    not.push((n.into(), "поздно вечером бодрит и мешает уснуть".into()));
                } else if heavy || (ran && tomorrow_hard) {
                    good.push((60, n.into(), "Ноги 10 минут в прохладной воде (10–15°) — меньше отёка и тяжести.".into()));
                }
            }
            "Растяжка" => {
                good.push((if ran || gym_h.is_some() { 55 } else { 35 }, n.into(), "10–15 минут: икры, задняя поверхность бедра, сгибатели бедра, грудной отдел.".into()));
            }
            "Мази" => {
                if let Some(p) = pains.iter().find(|p| p.2 >= 2 && p.2 <= 5) {
                    good.push((50, n.into(), format!("Локально на {} — но не чтобы бегать через боль: мазь убирает ощущение, а не причину.", p.1.to_lowercase())));
                }
            }
            _ => {}
        }
    }
    good.sort_by(|a, b| b.0.cmp(&a.0));
    json!({
        "have": have.len(),
        "best": good.iter().take(3).map(|(_, n, t)| json!({"name": n, "text": t})).collect::<Vec<_>>(),
        "more": good.iter().skip(3).map(|(_, n, t)| json!({"name": n, "text": t})).collect::<Vec<_>>(),
        "not": not.iter().map(|(n, t)| json!({"name": n, "text": t})).collect::<Vec<_>>(),
        "context": {"heavy": heavy, "ran": ran, "legs": legs, "stressHigh": stress_hi, "tomorrow": tomorrow},
    })
}

// ---------- goal and control tests ----------

fn parse_time(s: &str) -> Option<f64> {
    let p: Vec<f64> = s.trim().split(':').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
    match p.len() {
        3 => Some(p[0] * 3600.0 + p[1] * 60.0 + p[2]),
        2 => Some(p[0] * 60.0 + p[1]),
        _ => None,
    }
}

pub fn fmt_time(s: f64) -> String {
    let t = s.round() as i64;
    if t >= 3600 { format!("{}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60) } else { format!("{}:{:02}", t / 60, t % 60) }
}

/// Best predicted time for `dist` from efforts in (to − 42 days, to].
fn predict(acts: &[engine_svc::Cached], tz: i64, to: &str, dist: f64) -> Option<(f64, f64, f64, String)> {
    let from = add_days(to, -42);
    let mut best: Option<(f64, f64, f64, String)> = None;
    for a in acts {
        let d = day_of(a.start_utc, tz);
        if d.as_str() <= from.as_str() || d.as_str() > to {
            continue;
        }
        for (bd, bt) in a.best.iter().filter(|x| x.0 >= 5000.0 || x.0 >= dist * 0.9) {
            let t = riegel(*bt, *bd, dist);
            if best.as_ref().map(|b| t < b.0).unwrap_or(true) {
                best = Some((t, *bd, *bt, d.clone()));
            }
        }
    }
    best
}

pub fn goal(conn: &mut Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let td = engine_svc::today(&p);
    let tz = p.tz_offset_s;
    let rest = engine_svc::resting_hr(conn, &p);
    let acts = engine_svc::refresh_metrics(conn, &p, rest)?;
    let g = coach_svc::profile(conn)["goalRace"].clone();
    let dist = g["distanceKm"].as_f64().map(|x| x * 1000.0);
    let target = g["target"].as_str().and_then(parse_time);
    let mut trend = Vec::new();
    let mut now_pred = None;
    if let Some(d) = dist {
        for k in (0..13).rev() {
            let to = add_days(&td, -7 * k);
            if let Some(x) = predict(&acts, tz, &to, d) {
                trend.push(json!([to, x.0.round()]));
                if k == 0 {
                    now_pred = Some(x);
                }
            }
        }
    }
    let days_left = g["date"].as_str().and_then(|x| Some(day_num(x)? - day_num(&td)?));
    // Control tests: the best ≥5 km effort on days the plan had a test.
    let test_days: Vec<String> = conn
        .prepare("SELECT date FROM plan_days WHERE items LIKE '%\"test\"%' AND date <= ?1 ORDER BY date")
        .ok()
        .map(|mut st| {
            let v: Vec<String> = st.query_map(params![td], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
            v
        })
        .unwrap_or_default();
    let mut tests = Vec::new();
    for d in &test_days {
        let best = acts
            .iter()
            .filter(|a| day_of(a.start_utc, tz) == *d)
            .flat_map(|a| a.best.iter().filter(|b| (b.0 - 5000.0).abs() < 1.0).map(move |b| (a.id.clone(), b.1)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        tests.push(json!({"date": d, "time": best.as_ref().map(|b| b.1.round()), "timeText": best.as_ref().map(|b| fmt_time(b.1)), "activity": best.map(|b| b.0)}));
    }
    let (pred_s, from_d, from_t, from_date) = match &now_pred {
        Some((a, b, c, d)) => (Some(*a), Some(*b), Some(*c), Some(d.clone())),
        None => (None, None, None, None),
    };
    let gap = match (pred_s, target) {
        (Some(p), Some(t)) => Some(p - t),
        _ => None,
    };
    let weekly = trend.len() >= 4;
    let rate = if weekly {
        let a = trend.first().and_then(|x| x[1].as_f64());
        let b = trend.last().and_then(|x| x[1].as_f64());
        match (a, b) {
            (Some(a), Some(b)) => Some((b - a) / ((trend.len() - 1) as f64)),
            _ => None,
        }
    } else {
        None
    };
    let next_test = conn
        .query_row("SELECT date FROM plan_days WHERE items LIKE '%\"test\"%' AND date >= ?1 ORDER BY date LIMIT 1", params![td], |r| r.get::<_, String>(0))
        .ok();
    let verdict = match (gap, days_left, rate) {
        (Some(g), Some(dl), Some(r)) if dl > 0 => {
            let weeks = dl as f64 / 7.0;
            let expected = g + r * weeks;
            if g <= 0.0 {
                "Ты уже в графике: прогноз не хуже цели. Главное — не сорваться в объёмах и прийти на старт свежим.".to_string()
            } else if expected <= 0.0 {
                format!("При нынешнем темпе прогресса ({} в неделю) цель достижима к старту.", fmt_time(r.abs()))
            } else {
                format!("Пока не хватает около {} — при нынешнем темпе к старту останется ≈{}. Поможет регулярность и одна качественная тренировка в неделю.", fmt_time(g), fmt_time(expected))
            }
        }
        (Some(g), _, _) if g <= 0.0 => "Прогноз уже не хуже цели.".to_string(),
        (Some(g), _, _) => format!("До цели около {}.", fmt_time(g)),
        _ => String::new(),
    };
    Ok(json!({
        "goal": g, "distanceM": dist, "targetS": target, "daysLeft": days_left,
        "predS": pred_s.map(|x| x.round()), "predText": pred_s.map(fmt_time), "fromDistanceM": from_d, "fromTimeS": from_t.map(|x| x.round()), "fromDate": from_date,
        "gapS": gap.map(|x| x.round()), "trend": trend, "ratePerWeek": rate.map(|x| x.round()), "verdict": verdict,
        "tests": tests, "nextTest": next_test,
    }))
}

/// Short lines for the coach report and the Claude package.
pub fn lines(conn: &mut Connection) -> Vec<String> {
    let mut out = Vec::new();
    for p in pains(conn).iter().filter(|p| p["active"].as_bool().unwrap_or(false)) {
        out.push(format!("Боль: {} — {}/10 ({}), {} дн. из 14", p["name"].as_str().unwrap_or(""), p["level"], match p["trend"].as_str() { Some("better") => "лучше", Some("worse") => "хуже", Some("same") => "без изменений", _ => "впервые" }, p["days14"]));
    }
    let r = return_state(conn);
    if r["active"].as_bool().unwrap_or(false) {
        out.push(format!("Возврат к бегу ({}): шаг {} из {} — {}", r["areaName"].as_str().unwrap_or(""), r["stage"].as_i64().unwrap_or(0) + 1, STEPS.len(), r["stageText"].as_str().unwrap_or("")));
    }
    if let Ok(g) = goal(conn) {
        if let (Some(t), Some(pr)) = (g["goal"]["title"].as_str().or(g["goal"]["distanceKm"].as_f64().map(|_| "цель")), g["predText"].as_str()) {
            out.push(format!("Цель: {} {} км за {} к {} — прогноз сейчас {}", t, g["goal"]["distanceKm"], g["goal"]["target"].as_str().unwrap_or("?"), g["goal"]["date"].as_str().unwrap_or("?"), pr));
        }
    }
    out
}

pub fn state(conn: &mut Connection) -> Result<Value, String> {
    let s = engine_svc::summary(conn)?;
    let g = goal(conn)?;
    Ok(json!({
        "areas": AREAS.iter().map(|a| json!({"key": a.0, "name": a.1})).collect::<Vec<_>>(),
        "pains": pains(conn), "pending": pending(conn), "return": return_state(conn),
        "tools": tools(conn, &s), "goal": g,
    }))
}
