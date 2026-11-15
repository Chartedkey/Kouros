use crate::*;
use rusqlite::{functions::FunctionFlags, Connection};
use serde_json::json;
pub(crate) fn db() -> Connection {
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
#[test]
fn snapshot_includes_real_training_without_identifying_text() {
    let mut c = db();
    let now = store::now();
    c.execute("INSERT INTO activities(id,sport,name,description,start_utc,elapsed_s,moving_s,distance_m,source,file_path)VALUES('PRIVATE_ID','run','PRIVATE_ACTIVITY_NAME','PRIVATE_DESCRIPTION',?1,1800,1800,4000,'file','PRIVATE_PATH')",[now-86400]).unwrap();
    planning::switch(&c, "deepseek").unwrap();
    for kind in ["daily_report", "weekly_report", "training_plan"] {
        let s = ai_context::preview(&mut c, kind, None).unwrap();
        let text = s.to_string();
        for secret in [
            "PRIVATE_ID",
            "PRIVATE_ACTIVITY_NAME",
            "PRIVATE_DESCRIPTION",
            "PRIVATE_PATH",
        ] {
            assert!(!text.contains(secret));
        }
        let rows = s["facts"]["activity_history"]["value"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][1], "run");
        assert_eq!(rows[0][2], 30.0);
        assert_eq!(s["facts"]["activity_summary_30d"]["value"][0][1], 1);
        if kind != "daily_report" {
            assert!(s["facts"].get("hrv_history").is_some());
            assert!(s["facts"].get("rhr_history").is_some());
        }
        assert!(ai::payload(&s, "01234567890123456789012345678901", "deepseek-flash").is_ok());
    }
}
#[test]
fn new_medication_changes_local_restrictions_and_invalidates_preview() {
    let mut c = db();
    planning::switch(&c, "deepseek").unwrap();
    let s = ai_context::preview(&mut c, "training_plan", None).unwrap();
    let today = s["today"].as_str().unwrap();
    let end = engine::add_days(today, 3);
    assert!(!ai_context::forbidden(&c, today).contains(&"long".into()));
    c.execute("INSERT INTO meds(id,kind,name,schedule,course_from,course_to,extra)VALUES(kouros_id(),'medicine','PRIVATE_MEDICINE','course',?1,?2,'{\"group\":\"fluoroquinolone\"}')",rusqlite::params![today,end]).unwrap();
    assert!(ai_context::forbidden(&c, today).contains(&"long".into()));
    let mut cfg = ai::config(&c);
    cfg["consent"] = json!(true);
    ai::save_config(&c, &cfg).unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false)
        .unwrap_err()
        .contains("изменились"));
    let next = ai_context::preview(&mut c, "training_plan", None).unwrap();
    assert!(!next.to_string().contains("PRIVATE_MEDICINE"));
    assert!(next["date_limits"][today]["forbidden_kinds"]
        .as_array()
        .unwrap()
        .contains(&json!("long")));
}

#[test]
fn psychology_latest_today_overrides_earlier_answer_and_invalidates_snapshot() {
    let mut c = db();
    let today = engine_svc::today(&engine_svc::profile(&c));
    psych_svc::save(&c, "tqr", &json!([6]), Some(&today)).unwrap();
    let snap = ai_context::preview(&mut c, "daily_report", None).unwrap();
    assert_eq!(snap["allowed_today_tier"], 1);
    assert_eq!(snap["facts"]["psychology"]["value"]["tqr"]["current"], true);
    let before = planning::data_revision(&c);
    psych_svc::save(&c, "tqr", &json!([20]), Some(&today)).unwrap();
    assert!(planning::data_revision(&c) > before);
    let s = engine_svc::summary(&mut c).unwrap();
    assert_eq!(s["day"]["constraints"]["subjectiveLevel"], 0);
    assert_eq!(s["day"]["constraints"]["hasSurvey"], true);
    psych_svc::delete(&c, "tqr", &today).unwrap();
    psych_svc::save(&c, "tqr", &json!([6]), Some(&engine::add_days(&today, -1))).unwrap();
    let s = engine_svc::summary(&mut c).unwrap();
    assert_eq!(s["day"]["constraints"]["subjectiveLevel"], 0);
    assert_eq!(s["day"]["constraints"]["hasSurvey"], false);
    let snap = ai_context::preview(&mut c, "daily_report", None).unwrap();
    assert_eq!(
        snap["facts"]["psychology"]["value"]["tqr"]["current"],
        false
    );
}

#[test]
fn long_term_goals_extend_strategy_but_do_not_rewrite_accepted_days() {
    let mut c = db();
    planning::switch(&c, "deepseek").unwrap();
    let today = engine_svc::today(&engine_svc::profile(&c));
    plan_svc::set_day(&c, &today, &json!([{"kind":"walk","min":20}])).unwrap();
    let mut p = ai::training(&c);
    p["goals"] = json!([{"id":"pullup","title":"Научиться подтягиваться","baseline":"0 повторений","target":"5 повторений","date":"2027-06-01","primary":true}]);
    p["program_weeks"] = json!(24);
    ai::save_training(&c, &p).unwrap();
    let s = ai_context::preview(&mut c, "training_plan", None).unwrap();
    assert_eq!(
        engine::day_num(s["period"]["to"].as_str().unwrap()).unwrap()
            - engine::day_num(s["period"]["from"].as_str().unwrap()).unwrap(),
        167
    );
    assert_eq!(s["profile"]["goals"][0]["id"], "pullup");
    let payload = ai::payload(&s, "01234567890123456789012345678901", "deepseek-flash").unwrap();
    assert!(ai::input_bound(&payload["messages"]) <= ai::limits("training_plan").0);
    assert!(payload.to_string().contains("24"));
    let summary = engine_svc::summary(&mut c).unwrap();
    assert_eq!(summary["day"]["acceptedPlan"]["items"][0]["kind"], "walk");
    p["program_weeks"] = json!(53);
    assert!(ai::save_training(&c, &p).is_err());
}

#[test]
fn bad_current_self_report_flags_accepted_workout_without_replacing_it() {
    let mut c = db();
    let today = engine_svc::today(&engine_svc::profile(&c));
    plan_svc::set_day(&c, &today, &json!([{"kind":"home","min":40}])).unwrap();
    let items: String = c
        .query_row("SELECT items FROM plan_days WHERE date=?1", [&today], |r| {
            r.get(0)
        })
        .unwrap();
    psych_svc::save(&c, "tqr", &json!([7]), Some(&today)).unwrap();
    let s = engine_svc::summary(&mut c).unwrap();
    assert_eq!(s["day"]["acceptedPlan"]["needsReview"], true);
    let after: String = c
        .query_row("SELECT items FROM plan_days WHERE date=?1", [&today], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(after, items);
    psych_svc::delete(&c, "tqr", &today).unwrap();
    let mut brums = vec![0; 24];
    for i in 16..20 {
        brums[i] = 4;
    }
    psych_svc::save(&c, "brums", &json!(brums), Some(&today)).unwrap();
    let s = engine_svc::summary(&mut c).unwrap();
    assert_eq!(s["day"]["constraints"]["subjectiveLevel"], 2);
    assert_eq!(s["day"]["acceptedPlan"]["needsReview"], true);
}
