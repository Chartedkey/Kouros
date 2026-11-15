use crate::*;
use rusqlite::{functions::FunctionFlags, Connection};
use serde_json::json;
fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    db::migrate_fixture(&c);
    c.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    c.create_scalar_function("kouros_id", 0, FunctionFlags::SQLITE_UTF8, |_| {
        static N: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(100);
        Ok(N.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    })
    .unwrap();
    planning::install(&c).unwrap();
    peer_data::init(&c).unwrap();
    sources::install_view(&c).unwrap();
    workouts_svc::install_view(&c).unwrap();
    exercise_svc::load_custom(&c);
    c
}
#[test]
fn snapshots_work_with_empty_actual_services() {
    let mut c = db();
    for mode in ["kouros", "deepseek"] {
        planning::switch(&c, mode).unwrap();
        for kind in ["daily_report", "weekly_report", "training_plan"] {
            let s = ai_context::preview(&mut c, kind, None)
                .unwrap_or_else(|e| panic!("{mode} {kind}: {e}"));
            assert_eq!(s["planning_mode"], mode);
            assert!(!s["quality_data_ready"].as_bool().unwrap());
            assert!(ai::payload(&s, "01234567890123456789012345678901", "deepseek-flash").is_ok());
        }
    }
}
#[test]
fn no_parallel_local_plan_or_report_in_deepseek() {
    let mut c = db();
    planning::switch(&c, "deepseek").unwrap();
    let st = plan_svc::state(&mut c).unwrap();
    assert!(st["days"]
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["hasPlan"] == false));
    assert!(st["proposals"].as_array().unwrap().is_empty());
    assert!(coach_svc::day(&mut c).is_err());
    assert!(coach_svc::week(&mut c).is_err());
    let summary = engine_svc::summary(&mut c).unwrap();
    assert_eq!(summary["day"]["options"], json!([]));
    assert_eq!(summary["day"]["advice"]["title"], "План ещё не составлен");
    assert!(plan_svc::save_template_days(&c, &json!([]), true).is_err());
}
#[test]
fn accepted_plan_survives_both_mode_switches() {
    let mut c = db();
    let today = engine_svc::today(&engine_svc::profile(&c));
    plan_svc::set_day(&c, &today, &json!([{"kind":"walk","min":20}])).unwrap();
    for mode in ["deepseek", "kouros", "deepseek"] {
        planning::switch(&c, mode).unwrap();
        plan_svc::state(&mut c).unwrap();
        let s = engine_svc::summary(&mut c).unwrap();
        assert_eq!(
            s["day"]["acceptedPlan"]["items"],
            json!([{"kind":"walk","min":20.0}])
        );
        assert_eq!(s["day"]["options"], json!([]));
    }
}
#[test]
fn snapshot_never_contains_private_names_or_notes() {
    let mut c = db();
    db::set_setting(
        &c,
        "profile",
        &json!({"name":"PRIVATE_PERSON_NAME","birthYear":1990}).to_string(),
    )
    .unwrap();
    db::set_setting(&c, "coach_notes", "PRIVATE_NOTES").unwrap();
    let s = ai_context::preview(&mut c, "daily_report", None).unwrap();
    let text = s.to_string();
    assert!(!text.contains("PRIVATE_PERSON_NAME"));
    assert!(!text.contains("PRIVATE_NOTES"));
}
#[test]
fn unchanged_repeated_preview_reuses_snapshot() {
    let mut c = db();
    planning::switch(&c, "deepseek").unwrap();
    let a = ai_context::preview(&mut c, "daily_report", None).unwrap();
    let b = ai_context::preview(&mut c, "daily_report", None).unwrap();
    assert_eq!(a["snapshot_id"], b["snapshot_id"]);
}
#[test]
fn imported_input_invalidates_the_real_snapshot() {
    let mut c = db();
    let s = ai_context::preview(&mut c, "daily_report", None).unwrap();
    let mut cfg = ai::config(&c);
    cfg["consent"] = json!(true);
    ai::save_config(&c, &cfg).unwrap();
    let rev = planning::data_revision(&c);
    c.execute(
        "INSERT INTO health_metrics(metric,date,num)VALUES('weight','2026-11-15',75)",
        [],
    )
    .unwrap();
    assert!(planning::data_revision(&c) > rev);
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
}
#[test]
fn populated_health_and_food_keep_snapshot_within_budget() {
    let mut c = db();
    let today = engine_svc::today(&engine_svc::profile(&c));
    for i in 0..35 {
        let d = engine::add_days(&today, -i);
        for (metric, value, unit) in [
            ("sleep_analysis_core_seconds", 15000.0, "s"),
            ("sleep_analysis_deep_seconds", 4000.0, "s"),
            ("sleep_analysis_rem_seconds", 6000.0, "s"),
            ("sleep_analysis_awake_seconds", 900.0, "s"),
            ("resting_heart_rate_merged_avg", 55.0, "bpm"),
            ("hrv_rmssd_merged_avg", 60.0, "ms"),
            ("dietary_energy_consumed_total", 2200.0, "kcal"),
            ("dietary_protein_total", 130.0, "g"),
        ] {
            c.execute("INSERT INTO health_metrics(metric,date,num,unit,provider,source)VALUES(?1,?2,?3,?4,'APPLE','PRIVATE_RAW_FILENAME')",rusqlite::params![metric,d,value,unit]).unwrap();
        }
    }
    planning::switch(&c, "deepseek").unwrap();
    for k in ["daily_report", "weekly_report", "training_plan"] {
        let s = ai_context::preview(&mut c, k, None).unwrap();
        assert!(!s.to_string().contains("PRIVATE_RAW_FILENAME"));
        assert!(ai::payload(&s, "01234567890123456789012345678901", "deepseek-flash").is_ok());
    }
}
#[test]
fn manual_choice_in_deepseek_does_not_rebalance_future() {
    let mut c = db();
    let today = engine_svc::today(&engine_svc::profile(&c));
    let tomorrow = engine::add_days(&today, 1);
    plan_svc::set_day(&c, &tomorrow, &json!([{"kind":"long","min":90}])).unwrap();
    planning::switch(&c, "deepseek").unwrap();
    let r = plan_svc::choose(&mut c, &today, &json!([{"kind":"long","min":90}])).unwrap();
    assert_eq!(r["changes"], json!([]));
    let items: String = c
        .query_row(
            "SELECT items FROM plan_days WHERE date=?1",
            [tomorrow],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&items).unwrap()[0]["min"].as_f64(),
        Some(90.0)
    );
}
#[test]
fn ai_account_and_consent_settings_never_leave_in_peer_pages() {
    let c = db();
    for key in [
        "ai_balance",
        "ai_config",
        "ai_planner_mode",
        "ai_training_profile",
    ] {
        db::set_setting(&c, key, "private").unwrap();
    }
    let mut cursor = 0;
    loop {
        let p = peer_data::page(&c, cursor, None).unwrap();
        assert!(p
            .changes
            .iter()
            .all(|x| x.t != "settings" || !x.k[0].as_str().unwrap_or("").starts_with("ai_")));
        if p.done {
            break;
        }
        cursor = p.cursor;
    }
}
