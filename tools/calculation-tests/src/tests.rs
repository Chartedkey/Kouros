use super::*;
use engine::{add_days, training::{activity_metrics, ActInput}, Profile, Series};
use rusqlite::Connection;
use serde_json::json;

fn close(a: f64, b: f64) { assert!((a - b).abs() < 1e-7, "{a} != {b}"); }
fn run() -> ActInput { ActInput { id: "test".into(), sport: "run".into(), duration_s: 3600.0, avg_hr: Some(140.0), ..Default::default() } }
fn calc(a: &ActInput) -> engine::training::ActMetrics { activity_metrics(&Profile::default(), 60.0, a) }
fn fixture(previous: bool) -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    if previous { db::migrate_to_previous(&c); } else { db::migrate_fixture(&c); }
    c.execute_batch("CREATE VIEW va AS SELECT * FROM activities WHERE hidden=0;").unwrap();
    let tx = c.unchecked_transaction().unwrap();
    store::insert_activity(&tx, "a", Some(123), "strava", None, &parse::Parsed {
        sport: "run".into(), start_utc: 1794700000, elapsed_s: Some(3600.0), moving_s: Some(3600.0),
        avg_hr: Some(140.0), distance_m: Some(8000.0), ..Default::default()
    }).unwrap();
    tx.commit().unwrap(); c
}
fn refresh(c: &mut Connection) -> engine_svc::Cached {
    engine_svc::refresh_metrics(c, &Profile::default(), 60.0).unwrap().remove(0)
}
fn streams(hr: f64) -> parse::Streams {
    parse::Streams { t: (0..=3600).collect(), hr: vec![Some(hr); 3601], ..Default::default() }
}
fn seed_muscle(c: &Connection) {
    c.execute("INSERT OR REPLACE INTO muscle_cache(activity_id,version,data)VALUES('a','fixture','{}')", []).unwrap();
}
fn cache_count(c: &Connection) -> i64 { c.query_row("SELECT count(*) FROM activity_metrics", [], |r| r.get(0)).unwrap() }

#[test] fn complete_curves_keep_existing_load_and_zones() {
    let expected = calc(&run()).load;
    for step in [1, 15, 30] {
        let a = ActInput { hr: (0..=3600 / step).map(|i| (i * step, 140.0)).collect(), ..run() };
        let m = calc(&a); assert_eq!(m.method, "hr"); close(m.load, expected); close(m.zone_s.iter().sum(), 3600.0);
    }
}
#[test] fn sparse_curves_fall_back_to_average() {
    let expected = calc(&run()).load;
    for step in [31, 60, 300] {
        let a = ActInput { hr: (0..=3600 / step).map(|i| (i * step, 140.0)).collect(), ..run() };
        let m = calc(&a); assert_eq!(m.method, "avg_hr"); close(m.load, expected); close(m.zone_s.iter().sum(), 3600.0);
    }
}
#[test] fn a_minute_of_pulse_does_not_stand_for_an_hour() {
    let a = ActInput { hr: (0..=60).map(|i| (i, 140.0)).collect(), ..run() };
    let m = calc(&a); assert_eq!(m.method, "avg_hr"); close(m.load, calc(&run()).load);
    close(m.zone_s.iter().sum(), 3600.0);
}
#[test] fn incomplete_curve_without_average_is_an_estimate() {
    let a = ActInput { avg_hr: None, hr: (0..=60).map(|i| (i, 140.0)).collect(), ..run() };
    let m = calc(&a); assert_eq!(m.method, "estimate"); close(m.load, 84.0); close(m.zone_s.iter().sum(), 0.0);
}
#[test] fn coverage_boundary_is_explicit() {
    for (end, method) in [(3419, "avg_hr"), (3420, "hr")] {
        let a = ActInput { hr: (0..=end).map(|i| (i, 140.0)).collect(), ..run() };
        assert_eq!(calc(&a).method, method);
    }
}
#[test] fn actual_pause_is_not_bridged() {
    let a = ActInput { duration_s: 1800.0, hr: (0..=900).chain(2700..=3600).map(|i| (i, 140.0)).collect(), ..run() };
    let m = calc(&a); assert_eq!(m.method, "hr"); close(m.zone_s.iter().sum(), 1800.0);
    close(m.load, calc(&ActInput { duration_s: 1800.0, ..run() }).load);
}
#[test] fn elapsed_curve_cannot_exceed_moving_duration() {
    let a = ActInput { duration_s: 1800.0, hr: (0..=3600).map(|i| (i, 140.0)).collect(), ..run() };
    let m = calc(&a); assert_eq!(m.method, "avg_hr"); close(m.zone_s.iter().sum(), 1800.0);
}
#[test] fn malformed_timestamps_and_bad_pulse_do_not_inflate_load() {
    for kind in [0, 1, 2, 3] {
        let mut a = ActInput { hr: (0..=3600).map(|i| (i, 140.0)).collect(), ..run() };
        match kind { 0 => a.hr.swap(50, 51), 1 => a.hr[0].0 = -1, 2 => a.hr[50].0 = a.hr[49].0, _ => { for v in &mut a.hr { v.1 = f64::NAN; } } }
        let m = calc(&a); assert_eq!(m.method, "avg_hr"); close(m.load, calc(&run()).load);
    }
}
#[test] fn invalid_duration_cannot_create_load() {
    for duration_s in [0.0, -10.0, f64::NAN, f64::INFINITY] {
        let a = ActInput { duration_s, hr: (0..=3600).map(|i| (i, 140.0)).collect(), ..run() };
        close(calc(&a).load, 0.0);
    }
}
#[test] fn strength_rpe_still_overrides_pulse() {
    let a = ActInput { sport: "strength".into(), rpe: Some(5.0), hr: (0..=60).map(|i| (i, 140.0)).collect(), ..run() };
    let m = calc(&a); assert_eq!(m.method, "rpe"); close(m.load, 135.0);
}
#[test] fn late_streams_recompute_existing_metrics() {
    let mut c = fixture(false); let before = refresh(&mut c); seed_muscle(&c);
    store::set_streams(&c, "a", &streams(160.0)).unwrap();
    assert_eq!(cache_count(&c), 0);
    assert_eq!(c.query_row("SELECT count(*) FROM muscle_cache", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    let after = refresh(&mut c); assert_eq!(before.method, "avg_hr"); assert_eq!(after.method, "hr"); assert!(after.load > before.load);
}
#[test] fn replaced_deleted_and_identical_streams_handle_cache_correctly() {
    let mut c = fixture(false); store::set_streams(&c, "a", &streams(140.0)).unwrap();
    let before = refresh(&mut c); seed_muscle(&c);
    store::set_streams(&c, "a", &streams(140.0)).unwrap(); assert_eq!(cache_count(&c), 1);
    assert_eq!(c.query_row("SELECT count(*) FROM muscle_cache", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    // Simulates replacement through peer import, bypassing store::set_streams.
    c.execute("UPDATE activity_streams SET data=?1 WHERE activity_id='a'", [streams(160.0).to_json()]).unwrap();
    assert_eq!(cache_count(&c), 0); assert!(refresh(&mut c).load > before.load);
    c.execute("DELETE FROM activity_streams WHERE activity_id='a'", []).unwrap();
    assert_eq!(cache_count(&c), 0); assert_eq!(refresh(&mut c).method, "avg_hr");
}
#[test] fn input_edits_invalidate_but_notes_and_identical_values_do_not() {
    let mut c = fixture(false); refresh(&mut c);
    c.execute("UPDATE activities SET notes='личная заметка',avg_hr=140 WHERE id='a'", []).unwrap(); assert_eq!(cache_count(&c), 1);
    for sql in ["avg_hr=150", "moving_s=1800", "sport='strength'", "rpe=7", "distance_m=9000", "elapsed_s=3700", "elev_gain_m=50", "start_utc=start_utc+60", "has_streams=2"] {
        c.execute_batch(&format!("UPDATE activities SET {sql} WHERE id='a'")).unwrap();
        assert_eq!(cache_count(&c), 0, "{sql}"); refresh(&mut c);
    }
}
#[test] fn previous_engine_cache_is_rebuilt_once() {
    let mut c = fixture(false); refresh(&mut c);
    c.execute("UPDATE activity_metrics SET version='e1:old',load=0", []).unwrap();
    assert!(refresh(&mut c).load > 0.0);
    let key: String = c.query_row("SELECT version FROM activity_metrics", [], |r| r.get(0)).unwrap(); assert!(key.starts_with("e2:"));
    c.execute("UPDATE activity_metrics SET load=123", []).unwrap(); close(refresh(&mut c).load, 123.0);
}
#[test] fn migration_preserves_personal_records_and_existing_frozen_scores() {
    let c = fixture(true);
    c.execute("UPDATE activities SET rpe=6,notes='сохранить' WHERE id='a'", []).unwrap();
    c.execute("INSERT INTO settings(key,value)VALUES('complexes',?1)", [json!([{"id":"mine","items":[{"exId":"Pushups"}]}]).to_string()]).unwrap();
    c.execute("INSERT INTO strength_sets(activity_id,idx,exercise,reps)VALUES('a',0,'Моё упражнение',12)", []).unwrap();
    c.execute("INSERT INTO readiness_log(date,score,level,data)VALUES('2026-11-15',55,'yellow','{}')", []).unwrap();
    db::migrate_fixture(&c); db::migrate_fixture(&c); assert_eq!(db::schema_version(&c), 25);
    assert_eq!(c.query_row("SELECT notes FROM activities WHERE id='a'", [], |r| r.get::<_, String>(0)).unwrap(), "сохранить");
    assert_eq!(c.query_row("SELECT reps FROM strength_sets", [], |r| r.get::<_, i64>(0)).unwrap(), 12);
    assert_eq!(c.query_row("SELECT score FROM readiness_log", [], |r| r.get::<_, f64>(0)).unwrap(), 55.0);
    assert!(db::get_setting(&c, "complexes").unwrap().contains("mine"));
    c.execute("INSERT INTO activity_metrics VALUES('a','old',1,'avg_hr','[]','[]')", []).unwrap();
    store::set_streams(&c, "a", &streams(140.0)).unwrap(); assert_eq!(cache_count(&c), 0);
}
#[test] fn rolled_back_stream_edit_keeps_cached_result() {
    let mut c = fixture(false); refresh(&mut c);
    { let tx = c.transaction().unwrap(); store::set_streams(&tx, "a", &streams(160.0)).unwrap(); assert_eq!(cache_count(&tx), 0); }
    assert_eq!(cache_count(&c), 1); assert!(store::streams_json(&c, "a").is_none());
}
#[test] fn muscle_ratio_waits_for_all_35_calendar_days() {
    let today = "2026-11-15";
    for days in [0, 21, 27, 33, 34, 40] {
        let daily: Series = (0..=days).map(|k| (add_days(today, -k), 10.0)).collect();
        let (ratio, week, _) = muscle_svc::acwr(&daily, today);
        if days < 34 { assert!(ratio.is_none()); } else { close(ratio.unwrap(), 1.0); }
        if days >= 6 { close(week, 70.0); }
    }
}
#[test] fn heart_history_threshold_and_zero_baseline_are_preserved() {
    let today = "2026-11-15";
    for days in [26, 27] {
        let daily: Series = (0..=days).map(|k| (add_days(today, -k), 10.0)).collect();
        let series = engine::training::load_series(&daily, today);
        assert_eq!(series.last().unwrap().acwr.is_some(), days == 27);
    }
    let zero: Series = (0..35).map(|k| (add_days(today, -k), 0.0)).collect();
    assert!(muscle_svc::acwr(&zero, today).0.is_none());
}
