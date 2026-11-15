use crate::{ai, db, engine, planning, store};
use rusqlite::{functions::FunctionFlags, Connection};
use serde_json::{json, Value};
fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    db::migrate_fixture(&c);
    c.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    c.create_scalar_function("kouros_id", 0, FunctionFlags::SQLITE_UTF8, |_| {
        Ok(store::now() * 1000000 + rand_id())
    })
    .unwrap();
    planning::install(&c).unwrap();
    c
}
fn rand_id() -> i64 {
    static N: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
fn fixture(c: &Connection, mode: &str, kind: &str) -> Value {
    planning::switch(c, mode).unwrap();
    let mut cfg = ai::config(c);
    cfg["consent"] = json!(true);
    ai::save_config(c, &cfg).unwrap();
    let today = engine::day_of(store::now(), 0);
    let from = if kind == "training_plan" {
        engine::add_days(&today, 1)
    } else {
        today.clone()
    };
    let to = if kind == "training_plan" {
        engine::add_days(&from, 27)
    } else {
        from.clone()
    };
    let candidates = ai::candidates(c, &ai::training(c), false, false);
    ai::save_snapshot(c,json!({"today":today,"job_type":kind,"period":{"from":from,"to":to},"facts":{"sleep":{"value":25200,"unit":"seconds"}},"local_hour":10,"quality_data_ready":false,"allowed_today_tier":3,"date_limits":{},"current_days":{},"locked_days":{},"rates":{},"weekly_load_cap":null,"trained_today":false,"known_loads":{},"available_patterns":[],"complexes":{},"candidates":if kind=="daily_report"{json!({})}else{candidates}})).unwrap()
}
fn report(s: &Value, id: &str) -> Value {
    json!({"schema_version":"kouros.ai.v1","request_id":id,"snapshot_id":s["snapshot_id"],"planning_mode":s["planning_mode"],"planning_revision":s["planning_revision"],"profile_revision":s["profile_revision"],"plan_revision":s["plan_revision"],"job_type":s["job_type"],"period":s["period"],"headline":"Сводка дня","sections":[{"key":"sleep","severity":"info","text":"Сон по сводке.","fact_refs":["sleep"],"observations":[{"fact_ref":"sleep","value":25200,"unit":"seconds"}],"actions":[]}],"limitations":["Стадии приблизительны."],"proposal":null})
}
fn proposal(s: &Value) -> Value {
    let from = s["period"]["from"].as_str().unwrap();
    json!({"scope":"four_week_block","from":s["period"]["from"],"to":s["period"]["to"],"reason":"Спокойный старт","expected_plan_revision":s["plan_revision"],"days":(0..7).map(|i|json!({"date":engine::add_days(from,i),"reason":"Восстановление","sessions":[{"session_id":format!("s{i}"),"kind":"walk","location":"outdoors","duration_minutes":20,"warmup_minutes":0,"cooldown_minutes":0,"intensity_note":"Спокойно","exercises":[],"complex_refs":[]}]})).collect::<Vec<_>>(),"week_outline":(0..4).map(|i|json!({"index":i+1,"from":engine::add_days(from,i*7),"focus":"Поддержание формы","progression_condition":"По переносимости и новым данным"})).collect::<Vec<_>>()})
}
fn provider(v: &Value) -> Value {
    json!({"usage":{"prompt_tokens":1000,"completion_tokens":100},"choices":[{"finish_reason":"stop","message":{"content":v.to_string()}}]})
}
fn prepare(c: &Connection, s: &Value) -> String {
    ai::prepare(c, s["snapshot_id"].as_str().unwrap(), false).unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
#[test]
fn ai_payloads_fit_real_default_catalog() {
    for (mode, kind) in [
        ("kouros", "daily_report"),
        ("deepseek", "daily_report"),
        ("deepseek", "weekly_report"),
        ("deepseek", "training_plan"),
    ] {
        let c = db();
        let s = fixture(&c, mode, kind);
        let p = ai::payload(&s, "01234567890123456789012345678901", "deepseek-flash")
            .unwrap_or_else(|e| panic!("{mode} {kind}: {e}"));
        assert!(ai::input_bound(&p["messages"]) <= ai::limits(kind).0);
        assert_eq!(p["thinking"]["type"], "disabled");
    }
}
#[test]
fn ai_candidate_ids_are_real() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    assert!(s["candidates"].as_object().unwrap().len() > 10);
    for id in s["candidates"].as_object().unwrap().keys() {
        assert!(crate::exercise_svc::by_id(id).is_some());
    }
}
#[test]
fn ai_explicit_consent() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let mut cfg = ai::config(&c);
    cfg["consent"] = json!(false);
    ai::save_config(&c, &cfg).unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
}
#[test]
fn ai_exact_facts_and_unknown_fields() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let mut v = report(&s, "job");
    assert!(ai::validate_response(&s, &v, "job").is_ok());
    v["sections"][0]["observations"][0]["value"] = json!(999);
    assert!(ai::validate_response(&s, &v, "job").is_err());
    v = report(&s, "job");
    v["invented"] = json!(true);
    assert!(ai::validate_response(&s, &v, "job").is_err());
}
#[test]
fn ai_wrong_snapshot_and_unknown_fact() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let mut v = report(&s, "job");
    v["snapshot_id"] = json!("other");
    assert!(ai::validate_response(&s, &v, "job").is_err());
    v = report(&s, "job");
    v["sections"][0]["fact_refs"] = json!(["private_note"]);
    assert!(ai::validate_response(&s, &v, "job").is_err());
}
#[test]
fn ai_kouros_is_analysis_only() {
    let c = db();
    let s = fixture(&c, "kouros", "training_plan");
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
    let mut v = report(&s, "j");
    v["proposal"] = proposal(&s);
    assert!(ai::validate_response(&s, &v, "j").is_err());
}
#[test]
fn ai_roundtrip_mode_rejects_late_response() {
    let c = db();
    let s = fixture(&c, "deepseek", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    planning::switch(&c, "kouros").unwrap();
    planning::switch(&c, "deepseek").unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&report(&s, &j))).unwrap()["status"],
        "superseded"
    );
}
#[test]
fn ai_profile_change_rejects_late_response() {
    let c = db();
    let s = fixture(&c, "deepseek", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut p = ai::training(&c);
    p["goal"] = json!("Другая цель");
    ai::save_training(&c, &p).unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&report(&s, &j))).unwrap()["status"],
        "superseded"
    );
}
#[test]
fn ai_plan_change_invalidates_snapshot() {
    let c = db();
    let s = fixture(&c, "deepseek", "daily_report");
    c.execute(
        "INSERT INTO plan_days VALUES(?1,'[]','manual','',0,1)",
        [s["today"].as_str().unwrap()],
    )
    .unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
}
#[test]
fn ai_budget_is_reserved_atomically() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    let cfg = ai::config(&c);
    let spent = ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"]
        .as_f64()
        .unwrap();
    assert!(spent > 0.0);
    let mut cfg = cfg;
    cfg["budget_usd"] = json!(0.1);
    ai::save_config(&c, &cfg).unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), true).is_err());
    ai::cancel(&c, &j).unwrap();
    assert_eq!(
        ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"],
        0.0
    );
}
#[test]
fn ai_cancellation_after_send_preserves_cost() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    ai::cancel(&c, &j).unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&report(&s, &j))).unwrap()["status"],
        "cancelled"
    );
    assert!(
        ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"]
            .as_f64()
            .unwrap()
            > 0.0
    );
}
#[test]
fn ai_ready_result_reused_no_paid_retry() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    assert!(ai::take_request(&c, &j).is_err());
    ai::finish(&c, &j, &provider(&report(&s, &j))).unwrap();
    assert_eq!(
        ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).unwrap()["cached"],
        true
    );
}
#[test]
fn ai_truncated_output_never_ready() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut p = provider(&report(&s, &j));
    p["choices"][0]["finish_reason"] = json!("length");
    assert_eq!(ai::finish(&c, &j, &p).unwrap()["status"], "rejected");
}
#[test]
fn ai_accept_is_atomic_idempotent_and_not_completed() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    let mut v = report(&s, &j);
    v["proposal"] = proposal(&s);
    ai::validate_response(&s, &v, &j).unwrap();
    ai::take_request(&c, &j).unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&v)).unwrap()["status"],
        "ready"
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM plan_days", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    ai::accept(&c, &j).unwrap();
    ai::accept(&c, &j).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        7
    );
    for table in ["activities", "strength_sets"] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert!(!ai::prescription(&c, v["proposal"]["from"].as_str().unwrap()).is_null());
}
#[test]
fn ai_locked_day_cannot_change() {
    let c = db();
    let mut s = fixture(&c, "deepseek", "training_plan");
    let p = proposal(&s);
    let d = p["from"].as_str().unwrap();
    s["locked_days"][d] = json!([{"kind":"walk","min":10}]);
    assert!(ai::validate_proposal(&s, &p).is_err());
}
#[test]
fn ai_no_hard_work_without_data() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let mut p = proposal(&s);
    p["days"][0]["sessions"][0]["kind"] = json!("long");
    assert!(ai::validate_proposal(&s, &p).is_err());
}
#[test]
fn ai_unknown_exercise_and_wrong_dose_rejected() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let mut p = proposal(&s);
    p["days"][0]["sessions"][0]["kind"] = json!("ofp");
    let (eid, ex) = s["candidates"].as_object().unwrap().iter().next().unwrap();
    let e = json!({"exercise_id":eid,"role":"main","sets":2,"dose":{"unit":ex["unit"],"min":5,"max":8,"per_side":ex["per_side"]},"rest_seconds":30,"target_rpe":5,"equipment_ids":[],"external_load_kg":null,"alternative_ids":[]});
    p["days"][0]["sessions"][0]["exercises"] = json!([e]);
    ai::validate_proposal(&s, &p).unwrap();
    p["days"][0]["sessions"][0]["exercises"][0]["dose"]["per_side"] =
        json!(!ex["per_side"].as_bool().unwrap());
    assert!(ai::validate_proposal(&s, &p).is_err());
    p["days"][0]["sessions"][0]["exercises"][0]["exercise_id"] = json!("invented");
    assert!(ai::validate_proposal(&s, &p).is_err());
}
#[test]
fn ai_weights_not_guessed() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let mut p = proposal(&s);
    let (eid, ex) = s["candidates"].as_object().unwrap().iter().next().unwrap();
    p["days"][0]["sessions"][0]["exercises"] = json!([{"exercise_id":eid,"sets":2,"dose":{"unit":ex["unit"],"min":5,"max":8,"per_side":ex["per_side"]},"rest_seconds":30,"target_rpe":5,"equipment_ids":[],"external_load_kg":16,"alternative_ids":[]}]);
    assert!(ai::validate_proposal(&s, &p).is_err());
}
#[test]
fn ai_balance_keeps_provider_amounts_and_currencies() {
    let v = json!({"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"12.345678","granted_balance":"0.500000","topped_up_balance":"11.845678","ignored":"private"},{"currency":"CNY","total_balance":"0.00","granted_balance":"0.00","topped_up_balance":"0.00"}]});
    let b = ai::normalize_balance(&v, 123).unwrap();
    assert_eq!(b["checked"], 123);
    assert_eq!(b["balance_infos"][0]["total_balance"], "12.345678");
    assert_eq!(b["balance_infos"][1]["currency"], "CNY");
    assert!(b["balance_infos"][0].get("ignored").is_none());
}
#[test]
fn ai_balance_missing_is_unknown_not_zero() {
    for v in [
        json!({}),
        json!({"is_available":false,"balance_infos":[]}),
        json!({"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"NaN","granted_balance":"0","topped_up_balance":"0"}]}),
        json!({"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"1","granted_balance":"0"}]}),
    ] {
        assert!(ai::normalize_balance(&v, 123).is_err());
    }
}
#[test]
fn ai_locked_day_numeric_representation_is_preserved() {
    let c = db();
    let mut s = fixture(&c, "deepseek", "training_plan");
    let p = proposal(&s);
    let d = p["from"].as_str().unwrap();
    s["locked_days"][d] = json!([{"kind":"walk","min":20.0}]);
    ai::validate_proposal(&s, &p).unwrap();
}
#[test]
fn ai_accept_rolls_back_all_rows_on_storage_failure() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    let mut v = report(&s, &j);
    v["proposal"] = proposal(&s);
    ai::take_request(&c, &j).unwrap();
    ai::finish(&c, &j, &provider(&v)).unwrap();
    c.execute_batch("CREATE TRIGGER reject_prescription BEFORE INSERT ON ai_prescriptions BEGIN SELECT RAISE(ABORT,'synthetic disk failure'); END;").unwrap();
    assert!(ai::accept(&c, &j).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM plan_days", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT status FROM ai_jobs WHERE id=?1", [j], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "ready"
    );
}
#[test]
fn ai_prepared_request_cannot_send_an_expired_snapshot() {
    let c = db();
    let s = fixture(&c, "kouros", "daily_report");
    let j = prepare(&c, &s);
    c.execute("UPDATE ai_snapshots SET created=?1", [store::now() - 901])
        .unwrap();
    assert!(ai::take_request(&c, &j).is_err());
}
#[test]
fn ai_accept_rechecks_current_allowed_tier() {
    let c = db();
    let s = fixture(&c, "deepseek", "daily_report");
    let j = prepare(&c, &s);
    let mut v = report(&s, &j);
    let mut p = proposal(&s);
    p["scope"] = json!("day_adjustment");
    p["to"] = p["from"].clone();
    p["week_outline"] = json!([]);
    p["days"].as_array_mut().unwrap().truncate(1);
    p["days"][0]["sessions"][0]["kind"] = json!("easy");
    v["proposal"] = p;
    ai::take_request(&c, &j).unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&v)).unwrap()["status"],
        "ready"
    );
    assert!(ai::accept_with_tier(&c, &j, Some(1)).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM plan_days", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn ai_medication_edits_and_enable_switch_invalidate_requests() {
    let c = db();
    let s = fixture(&c, "deepseek", "daily_report");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    c.execute(
        "INSERT INTO meds(id,kind,name)VALUES(77,'medicine','Synthetic medicine')",
        [],
    )
    .unwrap();
    assert_eq!(
        ai::finish(&c, &j, &provider(&report(&s, &j))).unwrap()["status"],
        "superseded"
    );
    let s = fixture(&c, "deepseek", "daily_report");
    db::set_setting(&c, "meds_enabled", "0").unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
    let s = fixture(&c, "deepseek", "daily_report");
    c.execute("UPDATE meds SET archived=1 WHERE id=77", [])
        .unwrap();
    assert!(ai::prepare(&c, s["snapshot_id"].as_str().unwrap(), false).is_err());
}
#[test]
fn ai_home_fullbody_is_valid_without_fixed_availability() {
    let c = db();
    let mut s = fixture(&c, "deepseek", "training_plan");
    let j = "synthetic-home";
    let mut v = report(&s, j);
    let mut p = proposal(&s);
    let (eid, ex) = s["candidates"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, x)| x["equipment"] == "bodyweight")
        .unwrap();
    p["days"][0]["sessions"][0] = json!({"session_id":"home-session","kind":"home","location":"home","duration_minutes":90,"warmup_minutes":5,"cooldown_minutes":5,"intensity_note":"Спокойно","strength_format":"fullbody","split_part":null,"complex_refs":[],"exercises":[{"exercise_id":eid,"role":"main","sets":2,"dose":{"unit":ex["unit"],"min":8,"max":10,"per_side":ex["per_side"]},"rest_seconds":60,"target_rpe":5,"equipment_ids":[],"external_load_kg":null,"alternative_ids":[]}]});
    v["proposal"] = p;
    s["profile"]["max_minutes"] = json!([5, 5, 5, 5, 5, 5, 5]);
    ai::validate_response(&s, &v, j).unwrap();
    s["profile"]["strength_formats"]["home"] = json!("split");
    assert!(ai::validate_response(&s, &v, j).is_err());
}
#[test]
fn ai_preserves_user_transferred_locked_day_outside_usual_weekdays() {
    let c = db();
    let mut s = fixture(&c, "deepseek", "training_plan");
    let p = proposal(&s);
    let d = p["from"].as_str().unwrap();
    s["profile"]["days"] = json!([]);
    for day in p["days"].as_array().unwrap() {
        s["locked_days"][day["date"].as_str().unwrap()] = json!([{"kind":"walk","min":20.0}]);
    }
    assert!(s["locked_days"].get(d).is_some());
    ai::validate_proposal(&s, &p).unwrap();
}

#[test]
fn extended_strategy_validates_and_applies_only_first_seven_days() {
    for weeks in [1, 12, 24, 52] {
        let mut c = db();
        let mut s = fixture(&c, "deepseek", "training_plan");
        let from = s["period"]["from"].as_str().unwrap().to_string();
        s["period"]["to"] = json!(engine::add_days(&from, weeks * 7 - 1));
        let s = ai::save_snapshot(&c, s).unwrap();
        let mut p = proposal(&s);
        p["week_outline"]=json!((0..weeks).map(|i|json!({"index":i+1,"from":engine::add_days(&from,i*7),"focus":"База и постепенная адаптация","progression_condition":"Только при достаточном восстановлении"})).collect::<Vec<_>>());
        ai::validate_proposal(&s, &p).unwrap();
        let job = prepare(&c, &s);
        let mut r = report(&s, &job);
        r["proposal"] = p.clone();
        ai::take_request(&c, &job).unwrap();
        assert_eq!(
            ai::finish(&c, &job, &provider(&r)).unwrap()["status"],
            "ready"
        );
        ai::accept_with_tier(&mut c, &job, Some(3)).unwrap();
        let count: i64 = c
            .query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 7);
        p["week_outline"].as_array_mut().unwrap().pop();
        assert!(ai::validate_proposal(&s, &p).is_err());
    }
}

#[test]
fn provider_partial_week_is_kept_without_inventing_missing_days() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    let mut p = proposal(&s);
    p["days"].as_array_mut().unwrap().remove(2);
    r["proposal"] = p;
    assert_eq!(
        ai::finish(&c, &j, &provider(&r)).unwrap()["status"],
        "ready"
    );
    let saved = ai::state(&c, s["today"].as_str().unwrap()).unwrap();
    assert_eq!(
        saved["jobs"][0]["response"]["proposal"]["days"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(saved["jobs"][0]["validationNote"]
        .as_str()
        .unwrap()
        .contains("6 из первых 7"));
    ai::accept(&c, &j).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 6);
    assert_eq!(ai::raw_response(&c, &j).unwrap()["text"], r.to_string());
}

#[test]
fn provider_extra_weeks_are_preserved_raw_but_never_applied_as_detailed_days() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    let mut p = proposal(&s);
    let mut extra = p["days"][0].clone();
    extra["date"] = json!(engine::add_days(s["period"]["from"].as_str().unwrap(), 10));
    p["days"].as_array_mut().unwrap().push(extra);
    r["proposal"] = p;
    assert_eq!(
        ai::finish(&c, &j, &provider(&r)).unwrap()["status"],
        "ready"
    );
    ai::accept(&c, &j).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 7);
    let raw = ai::raw_response(&c, &j).unwrap();
    assert_eq!(raw["text"], r.to_string());
}

#[test]
fn rejected_paid_output_can_be_rechecked_offline_without_charging_or_applying() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    let mut p = proposal(&s);
    p["days"] = json!([]);
    r["proposal"] = p;
    let result = ai::finish(&c, &j, &provider(&r)).unwrap();
    assert_eq!(result["status"], "rejected");
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("Получено элементов: 0"));
    assert_eq!(ai::raw_response(&c, &j).unwrap()["text"], r.to_string());
    let spent = ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"].clone();
    assert_eq!(ai::recheck(&c, &j).unwrap()["status"], "rejected");
    assert_eq!(
        ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"],
        spent
    );
    assert!(ai::accept(&c, &j).is_err());
    planning::switch(&c, "kouros").unwrap();
    assert_eq!(ai::recheck(&c, &j).unwrap()["status"], "rejected");
    assert!(ai::accept(&c, &j).is_err());
}

#[test]
fn duplicate_dates_and_invalid_exercises_remain_rejected_and_preserved() {
    for duplicate in [true, false] {
        let c = db();
        let s = fixture(&c, "deepseek", "training_plan");
        let j = prepare(&c, &s);
        ai::take_request(&c, &j).unwrap();
        let mut r = report(&s, &j);
        let mut p = proposal(&s);
        if duplicate {
            p["days"][1]["date"] = p["days"][0]["date"].clone();
        } else {
            p["days"][0]["sessions"][0]["kind"] = json!("invented_activity");
        }
        r["proposal"] = p;
        assert_eq!(
            ai::finish(&c, &j, &provider(&r)).unwrap()["status"],
            "rejected"
        );
        assert!(ai::raw_response(&c, &j).is_ok());
        assert!(ai::accept(&c, &j).is_err());
    }
}

#[test]
fn saved_response_can_become_ready_after_local_parser_update_without_api() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    let mut p = proposal(&s);
    p["days"].as_array_mut().unwrap().remove(1);
    r["proposal"] = p;
    ai::finish(&c, &j, &provider(&r)).unwrap();
    c.execute("UPDATE ai_jobs SET status='rejected',response=NULL,error='Legacy exact-seven-days check' WHERE id=?1",[&j]).unwrap();
    let before = ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"].clone();
    assert_eq!(ai::recheck(&c, &j).unwrap()["status"], "ready");
    assert_eq!(
        ai::state(&c, s["today"].as_str().unwrap()).unwrap()["spent"],
        before
    );
    let count: i64 = c
        .query_row("SELECT count(*) FROM ai_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(ai::recheck(&c, &j).is_err());
}

#[test]
fn malformed_root_is_rejected_without_panicking_and_raw_text_survives() {
    for text in ["[]", "null", "42", "{\"proposal\":[]}", "{broken"] {
        let c = db();
        let s = fixture(&c, "deepseek", "training_plan");
        let j = prepare(&c, &s);
        ai::take_request(&c, &j).unwrap();
        let p = json!({"usage":{"prompt_tokens":100,"completion_tokens":20},"choices":[{"finish_reason":"stop","message":{"content":text}}]});
        assert_eq!(ai::finish(&c, &j, &p).unwrap()["status"], "rejected");
        assert_eq!(ai::raw_response(&c, &j).unwrap()["text"], text);
    }
}

// Regression shapes are anonymised. Never commit a user's health records or original response.
fn structured_snapshot(c: &Connection) -> Value {
    let mut s = fixture(c, "deepseek", "training_plan");
    let from = s["period"]["from"].as_str().unwrap().to_string();
    s["period"]["to"] = json!(engine::add_days(&from, 167));
    s["facts"]["hrv"] = json!({"value":[engine::add_days(&from,-1),65.5],"unit":"ms"});
    s["facts"]["nutrition"] =
        json!({"value":[{"date":engine::add_days(&from,-1),"protein":80}],"unit":"day aggregates"});
    s["facts"]["psychology"] = json!({"value":{"tqr":{"current":false}},"unit":"questionnaires"});
    s["weekly_load_cap"] = json!(500.0);
    ai::save_snapshot(c, s).unwrap()
}
fn outline_24(from: &str) -> Value {
    json!((0..24).map(|i|json!({"index":i+1,"from":engine::add_days(from,i*7),"focus":"Умеренная поддержка формы","progression_condition":"Пересмотреть по актуальному самочувствию"})).collect::<Vec<_>>())
}

#[test]
fn structured_facts_are_cited_without_model_copy_and_resolved_from_snapshot() {
    let c = db();
    let s = structured_snapshot(&c);
    let mut r = report(&s, "j");
    r["sections"][0]["fact_refs"] = json!(["hrv", "nutrition", "current_days"]);
    r["sections"][0]["observations"] =
        json!([{"fact_ref":"hrv"},{"fact_ref":"nutrition"},{"fact_ref":"current_days"}]);
    // This assertion only tests evidence; no proposal needed for a report fixture.
    r["job_type"] = json!("daily_report");
    let mut day = s.clone();
    day["job_type"] = json!("daily_report");
    ai::validate_response(&day, &r, "j").unwrap();
    let evidence = ai::fact_evidence(&s);
    assert_eq!(evidence["hrv"]["value"][1], 65.5);
    assert!(evidence["nutrition"]["value"].is_array());
    assert_eq!(evidence["current_days"]["value"], s["current_days"]);
    let payload = ai::payload(&s, "j", "deepseek-v4-pro").unwrap();
    let content: Value =
        serde_json::from_str(payload["messages"][1]["content"].as_str().unwrap()).unwrap();
    let observation = &content["response_schema"]["properties"]["sections"]["items"]["properties"]
        ["observations"]["items"];
    assert_eq!(observation["required"], json!(["fact_ref"]));
    assert!(observation["properties"].get("value").is_none());
    assert_eq!(
        content["response_schema"]["$defs"]["proposal"]["properties"]["scope"]["const"],
        "training_program"
    );
}

#[test]
fn realistic_legacy_mismatches_quarantine_analysis_and_keep_only_valid_explicit_day() {
    let c = db();
    let s = structured_snapshot(&c);
    let from = s["period"]["from"].as_str().unwrap();
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    r["sections"] = json!([
        {"key":"sleep","severity":"info","text":"Сон из исходной сводки","fact_refs":["sleep"],"observations":[{"fact_ref":"sleep","value":25200,"unit":"seconds"}],"actions":[]},
        {"key":"nutrition","severity":"attention","text":"Unverified deficit claim","fact_refs":["nutrition"],"observations":[{"fact_ref":"nutrition","value":"80 g of protein yesterday","unit":"aggregates"}],"actions":["Unverified action"]},
        {"key":"recovery","severity":"info","text":"Unverified recovery claim","fact_refs":["hrv","psychology"],"observations":[{"fact_ref":"hrv","value":65.5,"unit":"ms"},{"fact_ref":"psychology","value":"missing","unit":"date"}],"actions":["Unverified action"]},
        {"key":"training","severity":"info","text":"Ссылка на сохранённый план","fact_refs":["current_days","strength_schedule"],"observations":[{"fact_ref":"current_days"}],"actions":[]},
        {"key":"weekly_load","severity":"info","text":"Риск перетренированности низкий","fact_refs":["weekly_load_cap"],"observations":[{"fact_ref":"weekly_load_cap","value":500,"unit":"load"}],"actions":[]}
    ]);
    let mut p = proposal(&s);
    p["to"] = json!(engine::add_days(from, 27));
    p["days"] = json!([p["days"][0].clone()]);
    p["week_outline"] = outline_24(from);
    r["proposal"] = p;
    assert_eq!(
        ai::finish(&c, &j, &provider(&r)).unwrap()["status"],
        "ready"
    );
    let state = ai::state(&c, s["today"].as_str().unwrap()).unwrap();
    let job = &state["jobs"][0];
    let saved = &job["response"];
    assert_eq!(saved["sections"][0]["text"], "Сон из исходной сводки");
    assert!(!saved.to_string().contains("Unverified"));
    assert!(saved["sections"][1]["actions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(saved["proposal"]["to"], s["period"]["to"]);
    assert_eq!(saved["proposal"]["days"].as_array().unwrap().len(), 1);
    assert!(job["validationNote"]
        .as_str()
        .unwrap()
        .contains("1 из первых 7"));
    assert!(job["validationNote"]
        .as_str()
        .unwrap()
        .contains("Скрыты неподтверждённые"));
    assert_eq!(ai::raw_response(&c, &j).unwrap()["text"], r.to_string());
    ai::accept(&c, &j).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn horizon_mismatch_without_matching_full_outline_is_not_repaired() {
    let c = db();
    let s = structured_snapshot(&c);
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    let mut p = proposal(&s);
    p["to"] = json!(engine::add_days(s["period"]["from"].as_str().unwrap(), 27));
    r["proposal"] = p;
    assert_eq!(
        ai::finish(&c, &j, &provider(&r)).unwrap()["status"],
        "rejected"
    );
    assert!(ai::accept(&c, &j).is_err());
}

#[test]
fn saved_analysis_is_readable_after_mode_change_but_proposal_cannot_be_applied() {
    let c = db();
    let s = fixture(&c, "deepseek", "training_plan");
    let j = prepare(&c, &s);
    ai::take_request(&c, &j).unwrap();
    let mut r = report(&s, &j);
    r["proposal"] = proposal(&s);
    ai::finish(&c, &j, &provider(&r)).unwrap();
    c.execute(
        "UPDATE ai_jobs SET status='rejected',response=NULL WHERE id=?1",
        [&j],
    )
    .unwrap();
    planning::switch(&c, "kouros").unwrap();
    assert_eq!(ai::recheck(&c, &j).unwrap()["status"], "superseded");
    let state = ai::state(&c, s["today"].as_str().unwrap()).unwrap();
    assert!(state["jobs"][0]["response"].is_object());
    assert!(ai::accept(&c, &j).is_err());
    let n: i64 = c
        .query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
