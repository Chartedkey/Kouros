use crate::*;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
fn future(c: &Connection, n: i64) -> String {
    engine::add_days(&engine_svc::today(&engine_svc::profile(c)), n)
}
fn set(c: &Connection, d: &str, k: &str) {
    plan_svc::set_day(c, d, &json!([{"kind":k,"min":30}])).unwrap();
}
fn row(c: &Connection, d: &str) -> String {
    c.query_row("SELECT items FROM plan_days WHERE date=?1", [d], |r| {
        r.get(0)
    })
    .unwrap()
}
#[test]
fn profile_removes_old_time_limits_and_groups_running() {
    let c = tests::db();
    let mut old = ai::training(&c);
    old.as_object_mut().unwrap().remove("categories");
    old.as_object_mut().unwrap().remove("strength_formats");
    old["allowed_kinds"] = json!(["easy", "long", "tempo", "gym", "walk"]);
    old["max_minutes"] = json!([5, 5, 5, 5, 5, 5, 5]);
    old["preferred_time"] = json!("08:00");
    db::set_setting(&c, "ai_training_profile", &old.to_string()).unwrap();
    let p = ai::training(&c);
    assert!(p.get("max_minutes").is_none());
    assert!(p.get("preferred_time").is_none());
    assert!(p["categories"].as_array().unwrap().contains(&json!("run")));
    assert!(p["allowed_kinds"]
        .as_array()
        .unwrap()
        .contains(&json!("intervals")));
    ai::save_training(&c, &p).unwrap();
}
#[test]
fn local_home_split_alternates_and_fullbody_has_no_part() {
    let c = tests::db();
    let mut p = ai::training(&c);
    p["strength_formats"]["home"] = json!("split");
    ai::save_training(&c, &p).unwrap();
    let a = future(&c, 2);
    let b = future(&c, 4);
    set(&c, &a, "home");
    set(&c, &b, "home");
    assert_eq!(schedule::metadata(&c, &a)["home"]["split_part"], "upper");
    assert_eq!(schedule::metadata(&c, &b)["home"]["split_part"], "lower");
    let x = future(&c, 6);
    p["strength_formats"]["home"] = json!("fullbody");
    ai::save_training(&c, &p).unwrap();
    set(&c, &x, "home");
    assert!(schedule::metadata(&c, &x)["home"]["split_part"].is_null());
    assert_eq!(schedule::metadata(&c, &a)["home"]["split_part"], "upper");
}
#[test]
fn exchange_changes_only_two_days_and_keeps_actual_history() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let b = future(&c, 5);
    let other = future(&c, 8);
    set(&c, &a, "easy");
    set(&c, &b, "home");
    set(&c, &other, "walk");
    let untouched = row(&c, &other);
    let before_a = row(&c, &a);
    let before_b = row(&c, &b);
    let p = schedule::preview(&mut c, &a, &b, true).unwrap();
    assert_eq!(p["before"][&a]["items"][0]["kind"], "easy");
    assert!(p["canApply"].as_bool().unwrap(), "{p}");
    schedule::accept(&mut c, p["id"].as_str().unwrap()).unwrap();
    schedule::accept(&mut c, p["id"].as_str().unwrap()).unwrap();
    assert_eq!(row(&c, &a), before_b);
    assert_eq!(row(&c, &b), before_a);
    assert_eq!(row(&c, &other), untouched);
    for t in ["activities", "strength_sets"] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM plan_changes WHERE reason LIKE 'Обмен%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}
#[test]
fn moving_ai_prescription_preserves_exercises_and_source() {
    let mut c = tests::db();
    planning::switch(&c, "deepseek").unwrap();
    let a = future(&c, 2);
    let b = future(&c, 5);
    set(&c, &a, "ofp");
    let items: Value = serde_json::from_str(&row(&c, &a)).unwrap();
    c.execute("UPDATE plan_days SET source='deepseek' WHERE date=?1", [&a])
        .unwrap();
    let body = json!({"date":a,"sessions":[{"kind":"ofp","session_id":"stable-session","exercises":[{"exercise_id":"Bodyweight_Squat","sets":2,"dose":{"unit":"reps","min":8,"max":10}}]}]});
    c.execute(
        "INSERT INTO ai_prescriptions(date,job_id,items_hash,body)VALUES(?1,'old-job',?2,?3)",
        params![a, ai::hash(&items), body.to_string()],
    )
    .unwrap();
    let p = schedule::preview(&mut c, &a, &b, false).unwrap();
    assert_eq!(p["canApply"], true, "{p}");
    schedule::accept(&mut c, p["id"].as_str().unwrap()).unwrap();
    let moved = ai::prescription(&c, &b);
    assert_eq!(moved["date"], b);
    assert_eq!(moved["sessions"], body["sessions"]);
    assert!(ai::prescription(&c, &a).is_null());
}
#[test]
fn split_order_cannot_be_reversed_by_move() {
    let mut c = tests::db();
    let mut p = ai::training(&c);
    p["strength_formats"]["home"] = json!("split");
    ai::save_training(&c, &p).unwrap();
    let a = future(&c, 2);
    let b = future(&c, 5);
    let target = future(&c, 8);
    set(&c, &a, "home");
    set(&c, &b, "home");
    let edit = schedule::preview(&mut c, &a, &target, false).unwrap();
    assert_eq!(edit["canApply"], false);
    assert!(edit["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("последовательность")));
    assert!(schedule::accept(&mut c, edit["id"].as_str().unwrap()).is_err());
}
#[test]
fn heavy_neighbors_produce_warning_and_safe_date_options() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let b = future(&c, 5);
    let neighbor = future(&c, 6);
    set(&c, &a, "tempo");
    set(&c, &neighbor, "long");
    let p = schedule::preview(&mut c, &a, &b, false).unwrap();
    assert_eq!(p["canApply"], false);
    assert!(!p["alternatives"].as_array().unwrap().is_empty());
    assert!(schedule::accept(&mut c, p["id"].as_str().unwrap()).is_err());
}
#[test]
fn occupied_target_and_past_days_are_rejected() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let b = future(&c, 5);
    set(&c, &a, "walk");
    set(&c, &b, "easy");
    assert!(schedule::preview(&mut c, &a, &b, false).is_err());
    let past = future(&c, -1);
    assert!(schedule::preview(&mut c, &past, &b, true).is_err());
    assert!(schedule::preview(&mut c, &a, &a, true).is_err());
}
#[test]
fn edits_are_stale_after_mode_profile_or_medical_input_changes() {
    for variant in 0..3 {
        let mut c = tests::db();
        let a = future(&c, 2);
        let b = future(&c, 5);
        set(&c, &a, "walk");
        let p = schedule::preview(&mut c, &a, &b, false).unwrap();
        match variant {
            0 => {
                planning::switch(&c, "deepseek").unwrap();
            }
            1 => {
                let mut prefs = ai::training(&c);
                prefs["goal"] = json!("Новая цель");
                ai::save_training(&c, &prefs).unwrap();
            }
            _ => {
                db::set_setting(&c, "meds_enabled", "0").unwrap();
            }
        }
        assert!(schedule::accept(&mut c, p["id"].as_str().unwrap()).is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&row(&c, &a)).unwrap()[0]["kind"],
            "walk"
        );
    }
}
#[test]
fn failed_write_rolls_back_both_days_and_prescription() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let b = future(&c, 5);
    set(&c, &a, "walk");
    set(&c, &b, "easy");
    let before_a = row(&c, &a);
    let before_b = row(&c, &b);
    let p = schedule::preview(&mut c, &a, &b, true).unwrap();
    c.execute_batch(&format!("CREATE TRIGGER synthetic_failure BEFORE INSERT ON plan_days WHEN NEW.date='{b}' BEGIN SELECT RAISE(ABORT,'synthetic failure');END;")).unwrap();
    assert!(schedule::accept(&mut c, p["id"].as_str().unwrap()).is_err());
    assert_eq!(row(&c, &a), before_a);
    assert_eq!(row(&c, &b), before_b);
}
#[test]
fn fullbody_neighbor_requires_rest_but_complementary_home_split_can_move() {
    for split in [false, true] {
        let mut c = tests::db();
        let mut pref = ai::training(&c);
        pref["strength_formats"]["home"] = json!(if split { "split" } else { "fullbody" });
        ai::save_training(&c, &pref).unwrap();
        let a = future(&c, 2);
        let b = future(&c, 5);
        let target = future(&c, 4);
        set(&c, &a, "home");
        set(&c, &b, "home");
        let p = schedule::preview(&mut c, &a, &target, false).unwrap();
        assert_eq!(p["canApply"], split, "{p}");
    }
}
#[test]
fn manually_replaced_plan_does_not_move_stale_ai_exercises() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let b = future(&c, 5);
    set(&c, &a, "walk");
    c.execute("INSERT INTO ai_prescriptions(date,job_id,items_hash,body)VALUES(?1,'stale','wrong','{\"sessions\":[{\"kind\":\"home\",\"strength_format\":\"split\",\"split_part\":\"lower\"}]}')",[&a]).unwrap();
    let p = schedule::preview(&mut c, &a, &b, false).unwrap();
    assert_eq!(p["canApply"], true);
    schedule::accept(&mut c, p["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM ai_prescriptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn hard_home_prescription_is_checked_against_adjacent_heavy_run() {
    let mut c = tests::db();
    let a = future(&c, 2);
    let target = future(&c, 5);
    let neighbor = future(&c, 6);
    set(&c, &a, "home");
    set(&c, &neighbor, "long");
    let items: Value = serde_json::from_str(&row(&c, &a)).unwrap();
    c.execute("UPDATE plan_days SET source='deepseek' WHERE date=?1", [&a])
        .unwrap();
    c.execute("INSERT INTO ai_prescriptions VALUES(?1,'accepted',?2,?3)",params![a,ai::hash(&items),json!({"date":a,"sessions":[{"kind":"home","strength_format":"fullbody","session_id":"hard-home","exercises":[{"target_rpe":8}]}]}).to_string()]).unwrap();
    let p = schedule::preview(&mut c, &a, &target, false).unwrap();
    assert_eq!(p["canApply"], false);
    assert!(p["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x.as_str().unwrap().contains("тяжёлые занятия")));
}

#[test]
fn repeating_missed_session_preserves_past_and_rejects_duplicate_repeat() {
    let mut c = tests::db();
    planning::switch(&c, "deepseek").unwrap();
    let from = future(&c, -2);
    let to = future(&c, 3);
    c.execute(
        "INSERT INTO plan_days(date,items,source,ts,confirmed)VALUES(?1,?2,'chosen',0,1)",
        params![from, json!([{"kind":"walk","min":30}]).to_string()],
    )
    .unwrap();
    let past = row(&c, &from);
    let p = schedule::preview(&mut c, &from, &to, false).unwrap();
    assert_eq!(p["repeat"], true);
    assert_eq!(p["canApply"], true);
    schedule::accept(&mut c, p["id"].as_str().unwrap()).unwrap();
    assert_eq!(row(&c, &from), past);
    assert_eq!(row(&c, &to), past);
    let next = future(&c, 6);
    assert!(schedule::preview(&mut c, &from, &next, false).is_err());
}
