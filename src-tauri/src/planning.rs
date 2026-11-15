//! One planner for every view. Switching preserves accepted days and invalidates old work.
use crate::db;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub fn mode(c: &Connection) -> String {
    db::get_setting(c, "ai_planner_mode")
        .filter(|s| s == "deepseek")
        .unwrap_or_else(|| "kouros".into())
}
pub fn revision(c: &Connection) -> i64 {
    db::get_setting(c, "ai_planner_revision")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
pub fn local(c: &Connection) -> bool {
    mode(c) == "kouros"
}
pub fn state(c: &Connection) -> Value {
    json!({"mode":mode(c),"revision":revision(c)})
}
pub fn switch(c: &Connection, next: &str) -> Result<Value, String> {
    if !["kouros", "deepseek"].contains(&next) {
        return Err("Неизвестный автор плана".into());
    }
    if mode(c) == next {
        return Ok(state(c));
    }
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    let rev = revision(&tx) + 1;
    db::set_setting(&tx, "ai_planner_mode", next)?;
    db::set_setting(&tx, "ai_planner_revision", &rev.to_string())?;
    db::set_setting(&tx, "ai_switch_requires_confirm", "1")?;
    tx.execute(
        "UPDATE plan_changes SET state='superseded' WHERE state='proposed'",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE ai_jobs SET status='superseded' WHERE status='ready' AND applied=0",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(state(c))
}
pub fn data_revision(c: &Connection) -> i64 {
    c.query_row(
        "SELECT value FROM ai_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}
pub fn plan_revision(c: &Connection) -> String {
    use ring::digest::{digest, SHA256};
    let mut s = String::new();
    if let Ok(mut q) =
        c.prepare("SELECT date,items,source,COALESCE(confirmed,0) FROM plan_days ORDER BY date")
    {
        if let Ok(rows) = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        }) {
            for row in rows.flatten() {
                s.push_str(&format!("{:?}\n", row));
            }
        }
    }
    if let Ok(mut q) = c.prepare("SELECT date,body FROM ai_prescriptions ORDER BY date") {
        if let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        {
            for row in rows.flatten() {
                s.push_str(&format!("{:?}\n", row));
            }
        }
    }
    digest(&SHA256, s.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn install(c: &Connection) -> Result<(), String> {
    // Only input data; derived caches/logs must never invalidate their own snapshots.
    for table in [
        "activities",
        "activity_streams",
        "strength_sets",
        "health_metrics",
        "surveys",
        "checkins",
        "pain_log",
        "habit_log",
        "med_log",
        "meds",
        "lab_reports",
        "exercises",
        "lab_results",
        "plan_days",
        "ai_prescriptions",
        "plan_day_meta",
    ] {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS ai_revision_{table}_{op} AFTER {op} ON {table} BEGIN UPDATE ai_meta SET value=value+1 WHERE key='data_revision'; END;")).map_err(|e|e.to_string())?;
        }
    }
    for op in ["INSERT", "UPDATE", "DELETE"] {
        let row = if op == "DELETE" { "OLD" } else { "NEW" };
        c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS ai_settings_revision_{op} AFTER {op} ON settings WHEN {row}.key IN ('profile','coach_profile','data_sources','device_resets','sleep_schedule','complexes','custom_exercises','bands','meds_enabled','workout_sources','return_to_run','sleep_tonight','plan_template','habits_custom','lab_vo2') BEGIN UPDATE ai_meta SET value=value+1 WHERE key='data_revision'; END;")).map_err(|e|e.to_string())?;
    }
    c.execute(
        "UPDATE plan_changes SET state='superseded' WHERE state='proposed' AND origin='claude'",
        [],
    )
    .map_err(|e| e.to_string())?;
    c.execute("UPDATE ai_jobs SET status='uncertain', error='Приложение закрылось до получения ответа; возможна оплата запроса' WHERE status='sending'", []).map_err(|e|e.to_string())?;
    c.execute(
        "UPDATE ai_jobs SET status='cancelled',reserved=0 WHERE status='reserved'",
        params![],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
