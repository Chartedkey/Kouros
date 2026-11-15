//! Explicitly whitelisted summaries; never send raw files, GPS, identifiers or free-form health notes.
use crate::{
    ai,
    engine::{self, add_days},
    engine_svc, health_svc, plan_svc, store,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn forbidden(c: &Connection, date: &str) -> Vec<String> {
    let active = crate::body_svc::active(c);
    let lower = active.iter().any(|x| x.2 >= 4 && x.3);
    let upper = active.iter().any(|x| x.2 >= 4 && x.4);
    let restricted = crate::meds_svc::fq_until(c, date).is_some()
        || crate::labs_svc::crp_recent(c, date)
            .map(|(d, _)| date <= add_days(&d, 3).as_str())
            .unwrap_or(false);
    ai::KINDS
        .iter()
        .filter(|&&k| {
            (lower
                && matches!(
                    k,
                    "easy" | "walkrun" | "long" | "tempo" | "intervals" | "test"
                ))
                || (upper && matches!(k, "gym" | "upper" | "home"))
                || (restricted && matches!(k, "long" | "tempo" | "intervals" | "test"))
        })
        .map(|s| s.to_string())
        .collect()
}
fn fact(f: &mut Value, k: &str, v: Value, unit: &str, _source: &str) {
    f[k] = json!({"value":v,"unit":unit,"source":if k.starts_with("sleep")||k=="wakeups"{"sleep"}else if k.starts_with("nutrition"){"food"}else{"local"}});
}

pub fn preview(c: &mut Connection, kind: &str, start: Option<&str>) -> Result<Value, String> {
    if !["daily_report", "weekly_report", "training_plan"].contains(&kind) {
        return Err("Неизвестный вид запроса".into());
    }
    let plans = plan_svc::state(c)?;
    let s = engine_svc::summary(c)?;
    let h = health_svc::summary(c)?;
    let today = s["today"].as_str().ok_or("Нет локальной даты")?;
    let from = if kind == "training_plan" {
        start
            .map(String::from)
            .unwrap_or_else(|| add_days(today, 1))
    } else if kind == "weekly_report" {
        add_days(today, -7)
    } else {
        today.to_string()
    };
    if engine::day_num(&from).is_none()
        || (kind == "training_plan" && (from.as_str() < today || from > add_days(today, 28)))
    {
        return Err("Начало блока: сегодня или ближайшие 28 дней".into());
    }
    let to = if kind == "training_plan" {
        add_days(
            &from,
            ai::training(c)["program_weeks"]
                .as_i64()
                .unwrap_or(4)
                .clamp(1, 52)
                * 7
                - 1,
        )
    } else if kind == "weekly_report" {
        add_days(today, -1)
    } else {
        today.into()
    };
    let mut facts = json!({});
    let psych = crate::psych_svc::summary(c)?;
    let mut questionnaires = json!({});
    for kind in ["hooper", "tqr", "brums", "stressrec"] {
        let rows = psych[kind].as_array().cloned().unwrap_or_default();
        let latest = rows
            .iter()
            .filter(|r| r["date"].as_str().unwrap_or("") <= today)
            .max_by_key(|r| {
                (
                    r["date"].as_str().unwrap_or(""),
                    r["ts"].as_i64().unwrap_or(0),
                )
            });
        questionnaires[kind] = json!({"latest":latest,"current":latest.map(|r|r["date"]==today).unwrap_or(false),
            "history":rows.iter().filter(|r|r["date"].as_str().unwrap_or("")<=today).rev().take(7).collect::<Vec<_>>()});
    }
    fact(&mut facts,"psychology",questionnaires,"questionnaires","Личные ответы: проверяй дату; отсутствие ответа — неизвестно. BRUMS описывает настроение, не диагноз.");

    for (k, path, u) in [
        ("readiness", "/readiness/score", "points"),
        ("load_today", "/load/today", "load"),
        ("ctl", "/load/ctl", "load/day"),
        ("atl", "/load/atl", "load/day"),
        ("tsb", "/load/tsb", "load"),
        ("acwr", "/load/acwr", "ratio"),
        ("steps_today", "/steps/today", "steps"),
        ("weight", "/body/weight/smooth", "kg"),
    ] {
        fact(
            &mut facts,
            k,
            s.pointer(path).cloned().unwrap_or(Value::Null),
            u,
            "Kouros: импортированные данные и локальный расчёт",
        );
    }
    for (k, u) in [
        ("totalS", "seconds"),
        ("deepS", "seconds"),
        ("remS", "seconds"),
        ("coreS", "seconds"),
        ("awakeS", "seconds"),
        ("score", "points"),
        ("date", "date"),
    ] {
        fact(
            &mut facts,
            &format!("sleep_{k}"),
            h["night"][k].clone(),
            u,
            "Выбранный источник сна; стадии приблизительны",
        );
    }
    for (k, v, u) in [
        ("sleep_need", h["need"]["totalMin"].clone(), "minutes"),
        ("sleep_debt", h["need"]["debtMin"].clone(), "minutes"),
        ("wakeups", h["wakeups"]["count"].clone(), "count"),
        ("sleep_regularity", h["regularityMin"].clone(), "minutes"),
    ] {
        fact(&mut facts, k, v, u, "Kouros: сон");
    }
    for (key, unit) in [("hrv", "ms"), ("rhr", "bpm")] {
        let v = h[key]["last30"]
            .as_array()
            .and_then(|a| a.last())
            .cloned()
            .unwrap_or(Value::Null);
        fact(
            &mut facts,
            key,
            v,
            unit,
            "Выбранный источник; [дата, значение], не обязательно сегодня",
        );
    }
    let history_days = if kind == "daily_report" { 7 } else { 30 };
    let recent = |v: &Value, n: usize| -> Value {
        json!(v
            .as_array()
            .map(|a| a.iter().rev().take(n).cloned().collect::<Vec<_>>())
            .unwrap_or_default())
    };
    fact(
        &mut facts,
        "sleep_history",
        recent(&h["nights"], history_days),
        "night aggregates",
        "Выбранный источник сна",
    );
    let food = if kind == "weekly_report" {
        json!(s["nutrition"]["days"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|x| x["date"]
                .as_str()
                .map(|d| d >= from.as_str() && d <= to.as_str())
                .unwrap_or(false))
            .cloned()
            .collect::<Vec<_>>())
    } else {
        s["nutrition"]["days"].clone()
    };
    fact(
        &mut facts,
        "nutrition",
        recent(&food, history_days),
        "daily aggregates",
        "Импортированные записи питания: неполный дневник не означает дефицит",
    );
    fact(
        &mut facts,
        "nutrition_targets",
        s["nutrition"]["targetToday"].clone(),
        "daily targets",
        "Локальные ориентиры Kouros",
    );
    fact(
        &mut facts,
        "load_history",
        recent(&s["load"]["series"], history_days),
        "daily load",
        "Kouros",
    );
    // Real performed activities, with identifying text and routes removed.
    let p = engine_svc::profile(c);
    let rest = engine_svc::resting_hr(c, &p);
    let activities = engine_svc::refresh_metrics(c, &p, rest)?;
    let cutoff = add_days(today, -29);
    let mut summaries = std::collections::BTreeMap::<String, (usize, f64, f64)>::new();
    let mut activity_rows = vec![];
    for a in activities.iter().rev() {
        let date = engine::day_of(a.start_utc, p.tz_offset_s);
        if date < cutoff
            || date.as_str() > today
            || a.start_utc >= store::now()
            || !a.duration_s.is_finite()
            || a.duration_s <= 0.0
            || a.start_utc.saturating_add(a.duration_s as i64) > store::now()
        {
            continue;
        }
        let total = summaries.entry(a.sport.clone()).or_default();
        total.0 += 1;
        total.1 += a.duration_s / 60.0;
        total.2 += a.load;
        if activity_rows.len() < if kind == "daily_report" { 6 } else { 120 } {
            activity_rows.push(json!([
                date,
                a.sport,
                (a.duration_s / 60.0).round(),
                a.load.round(),
                a.method
            ]));
        }
    }
    fact(
        &mut facts,
        "activity_history",
        json!(activity_rows),
        "[date,sport,minutes,load,load_method]",
        "Kouros",
    );
    fact(
        &mut facts,
        "activity_summary_30d",
        json!(summaries
            .iter()
            .map(|(kind, (n, min, load))| json!([kind, n, min.round(), load.round()]))
            .collect::<Vec<_>>()),
        "[sport,count,minutes,load] over30days",
        "Kouros",
    );
    fact(
        &mut facts,
        "recovery_remaining",
        s["recovery"]["leftH"].clone(),
        "estimated hours; not physiological clearance",
        "Kouros",
    );
    for key in ["hrv", "rhr"] {
        fact(
            &mut facts,
            &format!("{key}_reference"),
            h[key]["band"].clone(),
            "personal reference range",
            "Kouros",
        );
    }
    if kind != "daily_report" {
        for key in ["hrv", "rhr"] {
            fact(
                &mut facts,
                &format!("{key}_history"),
                recent(&h[key]["last30"], 30),
                "[date,value]; ms forHRV,bpm forRHR",
                "Kouros",
            );
        }
        fact(
            &mut facts,
            "readiness_history",
            recent(&s["readiness"]["history"], 30),
            "[date,score]",
            "Kouros",
        );
    }
    let mut current = json!({});
    let mut strength_schedule = json!({});
    let mut locked = json!({});
    let mut q=c.prepare("SELECT date,items,source,COALESCE(confirmed,0) FROM plan_days WHERE date>=?1 AND date<=?2 ORDER BY date").map_err(|e|e.to_string())?;
    for row in q
        .query_map(
            params![
                today,
                if kind == "daily_report" {
                    add_days(today, 2)
                } else if kind == "training_plan" {
                    add_days(&from, 7)
                } else {
                    add_days(today, 14)
                }
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?
    {
        let (d, items, source, confirmed) = row.map_err(|e| e.to_string())?;
        let items: Value = serde_json::from_str(&items).map_err(|e| e.to_string())?;
        current[&d] = items.clone();
        let mut slots = crate::schedule::metadata(c, &d);
        let prescription = ai::prescription(c, &d);
        for session in prescription["sessions"].as_array().into_iter().flatten() {
            if let Some(kind) = session["kind"].as_str() {
                if matches!(kind, "home" | "gym") && session["strength_format"].is_string() {
                    slots[kind] = json!({"strength_format":session["strength_format"],"split_part":session["split_part"]});
                }
            }
        }
        if slots.as_object().map(|s| !s.is_empty()).unwrap_or(false) {
            strength_schedule[&d] = slots;
        }

        if confirmed == 1 || matches!(source.as_str(), "manual" | "chosen") {
            locked[&d] = items;
        }
    }
    drop(q);
    let active = crate::body_svc::active(c);
    let lower = active.iter().any(|x| x.2 >= 4 && x.3);
    let upper = active.iter().any(|x| x.2 >= 4 && x.4);
    let profile = ai::training(c);
    let mut candidates = if kind == "daily_report" {
        json!({})
    } else {
        ai::candidates(c, &profile, lower, upper)
    };
    let mut patterns: BTreeSet<String> = BTreeSet::new();
    if let Some(xs) = candidates.as_object() {
        for x in xs.values() {
            if let Some(p) = x["pattern"].as_str() {
                patterns.insert(p.into());
            }
        }
    }
    let mut date_limits = json!({});
    for i in 0..56 {
        let d = add_days(today, i);
        let fs = forbidden(c, &d);
        if !fs.is_empty() {
            date_limits[&d] = json!({"forbidden_kinds":fs});
        }
    }
    let mut known = json!({});
    let mut q=c.prepare("SELECT DISTINCT s.ex_id,s.weight_kg FROM strength_sets s JOIN activities a ON a.id=s.activity_id WHERE a.start_utc>=?1 AND s.ex_id IS NOT NULL AND s.weight_kg>0 AND s.reps>0").map_err(|e|e.to_string())?;
    for r in q
        .query_map([store::now() - 90 * 86400], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, w) = r.map_err(|e| e.to_string())?;
        if known.get(&id).is_none() {
            known[&id] = json!([]);
        }
        known[&id].as_array_mut().unwrap().push(json!(w));
    }
    drop(q);
    let mut completed = json!({});
    if let Some(xs) = s["load"]["series"].as_array() {
        for row in xs {
            if let Some(d) = row[0].as_str() {
                if d >= engine::training::week_start(today).as_str() && d <= today {
                    completed[d] = row[1].clone();
                }
            }
        }
    }
    let p = engine_svc::profile(c);
    let local_hour = (store::now() + p.tz_offset_s).rem_euclid(86400) / 3600;
    let mut body = json!({"provenance":{"sleep":"Selected sleep source; stages approximate. Check sleep_date for freshness.","food":"Imported food totals; incomplete diary is not a proven deficit.","local":"Selected sources and local Kouros calculation. HRV/RHR are [date,value]; check dates."},"job_type":kind,"today":today,"local_hour":local_hour,"as_of":store::now()/900*900,"period":{"from":from,"to":to},"facts":facts,"quality_data_ready":s["day"]["constraints"]["qualityDataReady"],"allowed_today_tier":s["day"]["constraints"]["tier"],"current_days":current,"strength_schedule":strength_schedule,"locked_days":locked,"date_limits":date_limits,"rates":plans["rates"],"weekly_load_cap":plans["week"]["cap"],"trained_today":s["day"]["todayActivities"].as_array().map(|xs|xs.iter().any(|x|x["sport"]!="walk")).unwrap_or(false),"candidates":candidates,"available_patterns":patterns,"known_loads":known,"completed_load":completed,"complexes":{},"limitations":["Не клиническая оценка. Стадии сна и восстановление приблизительны.","Дневник питания может быть неполным. Нет записи — не нулевое потребление.","Личные комплексы сохранены: новые программы выбирают отдельные упражнения."]});
    // Size validation happens before a job or paid request. Drop optional older history first.
    loop {
        let snapshot = ai::decorate_snapshot(c, body.clone());
        if ai::payload(
            &snapshot,
            "00000000000000000000000000000000",
            "deepseek-flash",
        )
        .is_ok()
        {
            return ai::save_snapshot(c, body);
        }
        let mut reduced = false;
        for key in [
            "sleep_history",
            "nutrition",
            "load_history",
            "activity_history",
            "hrv_history",
            "rhr_history",
            "readiness_history",
        ] {
            if let Some(a) = body["facts"][key]["value"].as_array_mut() {
                if a.len() > 1 {
                    a.pop();
                    reduced = true;
                }
            }
        }
        for kind in ["hooper", "tqr", "brums", "stressrec"] {
            if let Some(history) =
                body["facts"]["psychology"]["value"][kind]["history"].as_array_mut()
            {
                if !history.is_empty() {
                    history.pop();
                    reduced = true;
                }
            }
        }
        if !reduced {
            if let Some(o) = candidates.as_object_mut() {
                if o.len() > 5 {
                    let key = o.keys().last().cloned().unwrap();
                    o.remove(&key);
                    reduced = true;
                    body["available_patterns"] = json!(o
                        .values()
                        .filter_map(|x| x["pattern"].as_str())
                        .collect::<BTreeSet<_>>());
                    body["candidates"] = candidates.clone();
                }
            }
        }
        if !reduced {
            return Err(
                "Даже сокращённая сводка не помещается в лимит. Запрос не отправлен".into(),
            );
        }
    }
}
