//! Calendar edits move prescriptions, never completed activities. Every edit has a stale-data guard.
use crate::{
    ai, db,
    engine::{self, add_days},
    engine_svc, planning, store,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn read(c: &Connection, date: &str) -> Result<Option<Value>, String> {
    let row: Option<(String, String, Option<String>, i64)> = c
        .query_row(
            "SELECT items,source,note,COALESCE(confirmed,0) FROM plan_days WHERE date=?1",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(err)?;
    let Some((items, source, note, confirmed)) = row else {
        return Ok(None);
    };
    let airow: Option<(String, String, String)> = c
        .query_row(
            "SELECT job_id,items_hash,body FROM ai_prescriptions WHERE date=?1",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(err)?;
    let meta: Option<(String, String)> = c
        .query_row(
            "SELECT items_hash,body FROM plan_day_meta WHERE date=?1",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    let parsed: Value = serde_json::from_str(&items).map_err(err)?;
    let hash = ai::hash(&parsed);
    let airow = airow.filter(|x| source == "deepseek" && x.1 == hash);
    let meta = meta.filter(|x| x.0 == hash);
    Ok(Some(
        json!({"items":parsed,"source":source,"note":note,"confirmed":confirmed,"prescription":airow.map(|x|json!({"job_id":x.0,"items_hash":x.1,"body":serde_json::from_str::<Value>(&x.2).unwrap_or(Value::Null)})),"meta":meta.map(|x|json!({"items_hash":x.0,"body":serde_json::from_str::<Value>(&x.1).unwrap_or(Value::Null)}))}),
    ))
}
pub fn template(c: &Connection) -> Vec<Vec<crate::plan_svc::Item>> {
    let p = ai::training(c);
    let days = p["days"].as_array().cloned().unwrap_or_default();
    let cats = p["categories"].as_array().cloned().unwrap_or_default();
    let mut out = vec![vec![]; 7];
    if cats.is_empty() {
        return out;
    }
    let mut days: Vec<usize> = days
        .iter()
        .filter_map(|d| d.as_u64().map(|n| n as usize))
        .filter(|n| *n < 7)
        .collect();
    days.sort_unstable();
    days.dedup();
    // Spread selected directions over the available days. Durations are estimates, not availability limits.
    let order: Vec<&str> = ["run", "home", "ofp", "gym", "walk"]
        .into_iter()
        .filter(|k| cats.contains(&json!(k)))
        .collect();
    let run_count = (0..days.len())
        .filter(|i| order[*i % order.len()] == "run")
        .count();
    let mut run = 0;
    for (i, d) in days.iter().enumerate() {
        let category = order[i % order.len()];
        let (kind, min) = match category {
            "run" => {
                let k = if run_count >= 3 && run + 1 == run_count {
                    "long"
                } else if run_count >= 3 && run == 1 && p["experience"] != "novice" {
                    "tempo"
                } else {
                    "easy"
                };
                run += 1;
                (k, if k == "long" { 70.0 } else { 40.0 })
            }
            "home" => ("home", 40.0),
            "gym" => ("gym", 50.0),
            "ofp" => ("ofp", 25.0),
            _ => ("walk", 30.0),
        };
        out[*d].push(crate::plan_svc::Item {
            kind: kind.into(),
            min,
        });
    }
    out
}
pub fn profile_changed(c: &Connection) -> Result<(), String> {
    if !planning::local(c) {
        return Ok(());
    }
    let today = engine_svc::today(&engine_svc::profile(c));
    let days = template(c);
    let body = json!({"days":days.iter().map(|d|d.iter().map(|x|json!({"kind":x.kind,"min":x.min})).collect::<Vec<_>>()).collect::<Vec<_>>(),"auto":true,"start":engine::training::week_start(&today)});
    let tx = c.unchecked_transaction().map_err(err)?;
    db::set_setting(&tx, "plan_template", &body.to_string())?;
    tx.execute(
        "DELETE FROM plan_days WHERE date>?1 AND source='template' AND COALESCE(confirmed,0)=0",
        [today],
    )
    .map_err(err)?;
    tx.commit().map_err(err)
}
pub fn ensure_meta(c: &Connection, date: &str) -> Result<(), String> {
    let Some(row) = read(c, date)? else {
        return Ok(());
    };
    if row["source"] == "deepseek" {
        return Ok(());
    }
    let hash = ai::hash(&row["items"]);
    if row["meta"]["items_hash"] == hash {
        return Ok(());
    }
    let prefs = ai::training(c);
    let mut meta = json!({});
    for item in row["items"].as_array().into_iter().flatten() {
        let kind = item["kind"].as_str().unwrap_or("");
        if !["home", "gym"].contains(&kind) {
            continue;
        }
        let selected = prefs["strength_formats"][kind].as_str().unwrap_or("auto");
        let count = template(c)
            .iter()
            .flatten()
            .filter(|x| x.kind == kind)
            .count();
        let format = if selected == "auto" {
            if count >= 4 {
                "split"
            } else {
                "fullbody"
            }
        } else {
            selected
        };
        let previous:Option<String>=c.query_row("SELECT m.body FROM plan_day_meta m JOIN plan_days p ON p.date=m.date WHERE m.date<?1 AND json_extract(m.body,?2)='split' ORDER BY m.date DESC LIMIT 1",params![date,format!("$.{kind}.strength_format")],|r|r.get(0)).optional().map_err(err)?;
        let previous = previous.and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let part = if format == "split" {
            Some(
                if previous
                    .as_ref()
                    .map(|v| v[kind]["split_part"] == "upper")
                    .unwrap_or(false)
                {
                    "lower"
                } else {
                    "upper"
                },
            )
        } else {
            None
        };
        meta[kind] = json!({"session_id":format!("local-{date}-{kind}"),"strength_format":format,"split_part":part});
    }
    if !meta.as_object().unwrap().is_empty() {
        c.execute("INSERT INTO plan_day_meta(date,items_hash,body)VALUES(?1,?2,?3)ON CONFLICT(date)DO UPDATE SET items_hash=excluded.items_hash,body=excluded.body",params![date,hash,meta.to_string()]).map_err(err)?;
    }
    Ok(())
}
pub fn metadata(c: &Connection, date: &str) -> Value {
    c.query_row(
        "SELECT body FROM plan_day_meta WHERE date=?1",
        [date],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or(json!({}))
}
pub fn label(meta: &Value) -> String {
    if meta["strength_format"] == "split" {
        format!(
            "Сплит · {}",
            if meta["split_part"] == "upper" {
                "верх тела"
            } else {
                "низ тела"
            }
        )
    } else {
        "Фулбоди · всё тело".into()
    }
}
fn active(row: &Value) -> bool {
    row["items"]
        .as_array()
        .map(|xs| xs.iter().any(|x| x["kind"] != "rest"))
        .unwrap_or(false)
}
fn strengths(row: &Value) -> Vec<(String, String, String, String)> {
    let mut out = vec![];
    for x in row["prescription"]["body"]["sessions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if matches!(x["kind"].as_str(), Some("home" | "gym" | "upper" | "ofp")) {
            out.push((
                x["kind"].as_str().unwrap().into(),
                x["strength_format"].as_str().unwrap_or("fullbody").into(),
                x["split_part"].as_str().unwrap_or("").into(),
                format!("{}:{}", row["prescription"]["job_id"], x["session_id"]),
            ));
        }
    }
    if out.is_empty() {
        for x in row["items"].as_array().into_iter().flatten() {
            let k = x["kind"].as_str().unwrap_or("");
            if ["home", "gym", "upper", "ofp"].contains(&k) {
                let m = &row["meta"]["body"][k];
                out.push((
                    k.into(),
                    m["strength_format"].as_str().unwrap_or("fullbody").into(),
                    m["split_part"].as_str().unwrap_or("").into(),
                    m["session_id"].as_str().unwrap_or("").into(),
                ));
            }
        }
    }
    out
}
fn all(c: &Connection, today: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut q = c
        .prepare("SELECT date FROM plan_days WHERE date>=?1 ORDER BY date")
        .map_err(err)?;
    let ds = q
        .query_map([today], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut rows = BTreeMap::new();
    for d in ds {
        if let Some(r) = read(c, &d)? {
            rows.insert(d, r);
        }
    }
    Ok(rows)
}
fn patch(
    rows: &BTreeMap<String, Value>,
    from: &str,
    to: &str,
    swap: bool,
) -> BTreeMap<String, Value> {
    let mut result = rows.clone();
    let source = rows.get(from).cloned().unwrap_or(Value::Null);
    let destination = if swap {
        rows.get(to).cloned().unwrap_or(Value::Null)
    } else {
        json!({"items":[{"kind":"rest","min":0}],"source":"manual","note":"После переноса занятия","confirmed":1,"prescription":null,"meta":null})
    };
    result.insert(to.into(), source);
    result.insert(from.into(), destination);
    result
}
fn max_effort(row: &Value) -> f64 {
    row["prescription"]["body"]["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["exercises"].as_array().into_iter().flatten())
        .filter_map(|e| e["target_rpe"].as_f64())
        .fold(0.0, f64::max)
}
fn check(
    c: &Connection,
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
    from: &str,
    to: &str,
    summary: &Value,
) -> Vec<String> {
    let mut warnings = BTreeSet::new();
    let today = summary["today"].as_str().unwrap_or("");
    for date in [from, to] {
        let Some(row) = after.get(date) else { continue };
        let forbidden = crate::ai_context::forbidden(c, date);
        for x in row["items"].as_array().into_iter().flatten() {
            let k = x["kind"].as_str().unwrap_or("rest");
            if forbidden.contains(&k.to_string()) {
                warnings.insert(format!(
                    "{date}: занятие противоречит текущим ограничениям здоровья."
                ));
            }
            if date == today
                && ai::tier(k)
                    > summary["day"]["constraints"]["tier"].as_u64().unwrap_or(3) as usize
            {
                warnings.insert("Сегодня готовность не позволяет такую интенсивность.".into());
            }
            if date == today
                && summary["day"]["todayActivities"]
                    .as_array()
                    .map(|a| a.iter().any(|x| x["sport"] != "walk"))
                    .unwrap_or(false)
                && ai::tier(k) > 1
            {
                warnings.insert(
                    "Сегодня уже есть выполненная тренировка: перенеси занятие на другой день."
                        .into(),
                );
            }
        }
        if !strengths(row).is_empty() {
            for a in summary["day"]["recent"].as_array().into_iter().flatten() {
                let sport = a["sport"].as_str().unwrap_or("");
                if ["strength", "workout", "crossfit"].contains(&sport) {
                    let performed = engine::day_of(
                        a["startUtc"].as_i64().unwrap_or(0),
                        engine_svc::profile(c).tz_offset_s,
                    );
                    let gap = engine::day_num(date).unwrap_or(0)
                        - engine::day_num(&performed).unwrap_or(0);
                    if (0..2).contains(&gap) {
                        warnings.insert(format!("{date}: недавно была выполнена силовая тренировка. Выбери день с большим запасом восстановления."));
                    }
                }
            }
        }
        if date == today
            && summary["day"]["constraints"]["tier"].as_u64().unwrap_or(3) <= 3
            && max_effort(row) > 5.0
        {
            warnings.insert("Сегодня готовность требует более лёгких подходов. Перенеси занятие на другой день или пересмотри его усилие отдельно.".into());
        }
        for delta in [-1, 1] {
            let neighbor = add_days(date, delta);
            let Some(other) = after.get(&neighbor) else {
                continue;
            };
            let heavy = |r: &Value| {
                r["items"]
                    .as_array()
                    .map(|a| {
                        a.iter().any(|x| {
                            matches!(
                                x["kind"].as_str(),
                                Some("long" | "tempo" | "intervals" | "test" | "gym")
                            )
                        })
                    })
                    .unwrap_or(false)
                    || max_effort(r) >= 7.0
            };
            if heavy(row) && heavy(other) {
                warnings.insert(format!("{date} и {neighbor}: тяжёлые занятия подряд. Нужен более лёгкий день между ними."));
            }
            let a = strengths(row);
            let b = strengths(other);
            if a.iter().any(|(_, f, p, _)| {
                b.iter()
                    .any(|(_, g, q, _)| f != "split" || g != "split" || p == q)
            }) {
                warnings.insert(format!("{date} и {neighbor}: силовые на пересекающиеся группы мышц; оставь день восстановления."));
            }
        }
    }
    for k in ["home", "gym"] {
        let sequence = |rows: &BTreeMap<String, Value>| {
            rows.values()
                .flat_map(strengths)
                .filter(|x| x.0 == k && x.1 == "split")
                .map(|x| x.3)
                .collect::<Vec<_>>()
        };
        if sequence(before) != sequence(after) {
            warnings.insert(format!(
                "Нарушится последовательность сплита {}. Сохрани очередность верх / низ.",
                if k == "home" {
                    "дома"
                } else {
                    "в зале"
                }
            ));
        }
    }
    let cap = summary["load"]["ctl"]
        .as_f64()
        .filter(|n| *n > 5.0)
        .map(|n| n * 7.0 * engine_svc::profile(c).week_cap);
    if let Some(cap) = cap {
        let affected: BTreeSet<String> = [from, to]
            .iter()
            .map(|d| engine::training::week_start(d))
            .collect();
        for week in affected {
            let mut load = 0.0;
            for r in summary["load"]["series"].as_array().into_iter().flatten() {
                let d = r[0].as_str().unwrap_or("");
                if d <= today && engine::training::week_start(d) == week {
                    load += r[1].as_f64().unwrap_or(0.0);
                }
            }
            for (d, row) in after {
                if engine::training::week_start(d) != week {
                    continue;
                }
                for x in row["items"].as_array().into_iter().flatten() {
                    let k = x["kind"].as_str().unwrap_or("rest");
                    let rate = if k == "rest" {
                        0.0
                    } else if k == "walk" {
                        0.5
                    } else {
                        1.4
                    };
                    load += x["min"].as_f64().unwrap_or(0.0) * rate;
                }
            }
            let old_plan: f64 = before
                .iter()
                .filter(|(d, _)| engine::training::week_start(d) == week)
                .flat_map(|(_, r)| r["items"].as_array().into_iter().flatten())
                .map(|x| {
                    x["min"].as_f64().unwrap_or(0.0)
                        * match x["kind"].as_str().unwrap_or("") {
                            "rest" => 0.0,
                            "walk" => 0.5,
                            _ => 1.4,
                        }
                })
                .sum();
            let new_plan: f64 = after
                .iter()
                .filter(|(d, _)| engine::training::week_start(d) == week)
                .flat_map(|(_, r)| r["items"].as_array().into_iter().flatten())
                .map(|x| {
                    x["min"].as_f64().unwrap_or(0.0)
                        * match x["kind"].as_str().unwrap_or("") {
                            "rest" => 0.0,
                            "walk" => 0.5,
                            _ => 1.4,
                        }
                })
                .sum();
            if load > cap && new_plan > old_plan + 0.01 {
                warnings.insert(format!("Неделя с {week}: ожидаемая нагрузка превышает локальный ориентир ({:.0} > {:.0}). Выбери дату другой недели или сократи занятие отдельно.",load,cap));
            }
        }
    }
    warnings.into_iter().collect()
}
pub fn preview(c: &mut Connection, from: &str, to: &str, swap: bool) -> Result<Value, String> {
    let summary = engine_svc::summary(c)?;
    let today = summary["today"].as_str().ok_or("Нет даты")?;
    for d in [from, to] {
        if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err()
            || d < today
            || d > add_days(today, 21).as_str()
        {
            return Err("Выбери даты в ближайшие три недели; историю менять нельзя".into());
        }
    }
    if from == to {
        return Err("Выбери другой день".into());
    }
    if [from, to].contains(&today)
        && summary["day"]["todayActivities"]
            .as_array()
            .map(|a| a.iter().any(|x| x["sport"] != "walk"))
            .unwrap_or(false)
    {
        return Err("Сегодня уже зарегистрирована тренировка. Измени оставшийся план отдельно, чтобы не перенести выполненную часть.".into());
    }
    ensure_meta(c, from)?;
    ensure_meta(c, to)?;
    let rows = all(c, today)?;
    if !rows.get(from).map(active).unwrap_or(false) {
        return Err("В исходном дне нет тренировки".into());
    }
    if !swap && rows.get(to).map(active).unwrap_or(false) {
        return Err("Этот день занят. Выбери обмен тренировками".into());
    }
    if swap && !rows.get(to).map(active).unwrap_or(false) {
        return Err("Для обмена выбери день с тренировкой".into());
    }
    let after = patch(&rows, from, to, swap);
    let warnings = check(c, &rows, &after, from, to, &summary);
    let mut alternatives = vec![];
    if !warnings.is_empty() {
        for i in 1..=21 {
            let date = add_days(today, i);
            if date == from || date == to {
                continue;
            }
            let occupied = rows.get(&date).map(active).unwrap_or(false);
            if occupied != swap {
                continue;
            }
            let candidate = patch(&rows, from, &date, swap);
            if check(c, &rows, &candidate, from, &date, &summary).is_empty() {
                alternatives.push(date);
                if alternatives.len() == 5 {
                    break;
                }
            }
        }
    }
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(err)?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let body = json!({"id":id,"today":today,"from":from,"to":to,"swap":swap,"before":{(from):rows.get(from),(to):rows.get(to)},"after":{(from):after.get(from),(to):after.get(to)},"warnings":warnings,"alternatives":alternatives,"canApply":warnings.is_empty(),"plan_revision":planning::plan_revision(c),"data_revision":planning::data_revision(c),"planner":planning::state(c),"profile_revision":ai::profile_revision(c)});
    c.execute(
        "INSERT INTO plan_edits(id,created,body)VALUES(?1,?2,?3)",
        params![id, store::now(), body.to_string()],
    )
    .map_err(err)?;
    Ok(body)
}
fn write(c: &Connection, date: &str, row: &Value) -> Result<(), String> {
    c.execute("DELETE FROM plan_days WHERE date=?1", [date])
        .map_err(err)?;
    c.execute(
        "INSERT INTO plan_days(date,items,source,note,ts,confirmed)VALUES(?1,?2,?3,?4,?5,1)",
        params![
            date,
            row["items"].to_string(),
            if row["source"] == "deepseek" {
                "deepseek"
            } else {
                "chosen"
            },
            row["note"].as_str().unwrap_or("Перенесено тобой"),
            store::now()
        ],
    )
    .map_err(err)?;
    if row["prescription"].is_object() {
        let p = &row["prescription"];
        let mut body = p["body"].clone();
        body["date"] = json!(date);
        c.execute(
            "INSERT INTO ai_prescriptions(date,job_id,items_hash,body)VALUES(?1,?2,?3,?4)",
            params![
                date,
                p["job_id"].as_str(),
                p["items_hash"].as_str(),
                body.to_string()
            ],
        )
        .map_err(err)?;
    }
    if row["meta"].is_object() {
        let m = &row["meta"];
        c.execute(
            "INSERT INTO plan_day_meta(date,items_hash,body)VALUES(?1,?2,?3)",
            params![date, m["items_hash"].as_str(), m["body"].to_string()],
        )
        .map_err(err)?;
    }
    Ok(())
}
pub fn accept(c: &mut Connection, id: &str) -> Result<(), String> {
    let (created, text, applied): (i64, String, i64) = c
        .query_row(
            "SELECT created,body,applied FROM plan_edits WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    if applied == 1 {
        return Ok(());
    }
    let p: Value = serde_json::from_str(&text).map_err(err)?;
    let summary = engine_svc::summary(c)?;
    let today = summary["today"].as_str().unwrap_or("");
    if store::now() - created > 900
        || p["today"] != today
        || p["plan_revision"] != planning::plan_revision(c)
        || p["data_revision"].as_i64() != Some(planning::data_revision(c))
        || p["planner"] != planning::state(c)
        || p["profile_revision"] != ai::profile_revision(c)
    {
        return Err("Данные или план изменились. Проверь перенос заново".into());
    }
    let from = p["from"].as_str().ok_or("Нет исходного дня")?;
    let to = p["to"].as_str().ok_or("Нет нового дня")?;
    let rows = all(c, today)?;
    let after = patch(&rows, from, to, p["swap"] == true);
    let warnings = check(c, &rows, &after, from, to, &summary);
    if !warnings.is_empty() {
        return Err(warnings.join(" "));
    }
    let tx = c.unchecked_transaction().map_err(err)?;
    db::set_setting(&tx, "ai_switch_requires_confirm", "1")?;
    tx.execute(
        "UPDATE plan_changes SET state='superseded' WHERE state='proposed'",
        [],
    )
    .map_err(err)?;
    tx.execute(
        "UPDATE ai_jobs SET status='superseded' WHERE status='ready' AND applied=0",
        [],
    )
    .map_err(err)?;
    for date in [from, to] {
        let row = after.get(date).ok_or("Нет занятия")?;
        write(&tx, date, row)?;
        tx.execute("INSERT INTO plan_changes(id,ts,date,before,after,reason,origin,state)VALUES(kouros_id(),?1,?2,?3,?4,?5,'manual','accepted')",params![store::now(),date,rows.get(date).map(|x|x["items"].to_string()).unwrap_or_else(||"[]".into()),row["items"].to_string(),format!("{}: {from} ↔ {to}",if p["swap"]==true{"Обмен тренировками"}else{"Перенос тренировки"})]).map_err(err)?;
    }
    tx.execute("UPDATE plan_edits SET applied=1 WHERE id=?1", [id])
        .map_err(err)?;
    tx.commit().map_err(err)
}
