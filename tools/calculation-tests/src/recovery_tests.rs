use crate::{engine::{self, daily, training::LoadDay, Series}, engine_svc::Cached, muscle_svc::{self, MLoad}};
use serde_json::{json, Value};
use rusqlite::Connection;

const NOW: i64 = 1794700800; // Fixed clock; no dependence on test execution speed.
fn background() -> (Vec<LoadDay>, Series) {
    let today = engine::day_of(NOW, 0);
    let dates: Vec<_> = (-200..=10).map(|k| engine::add_days(&today, k)).collect();
    (dates.iter().map(|d| LoadDay {date:d.clone(), ctl:40.0, ..Default::default()}).collect(),
        dates.iter().map(|d| (d.clone(),20.0)).collect())
}
fn session(id: &str, end: i64) -> (Cached, MLoad) {
    let a=Cached {id:id.into(),sport:"run".into(),start_utc:end-3600,duration_s:3600.0,
        distance_m:Some(8000.0),calories:None,load:40.0,method:"hr".into(),zone_s:[0.0;6],best:vec![]};
    let m=MLoad {id:id.into(),sport:"run".into(),start:a.start_utc,end,total:20.0,
        groups:[0.0;12],up:0.0,down:0.0,bad:false,free:20.0};
    (a,m)
}
fn calc(acts:&[Cached], ml:&[MLoad], ready:Option<f64>, now:i64) -> Value {
    let (ls,md)=background();muscle_svc::recovery_at(acts,ml,&ls,&md,0,ready,now)
}

#[test] fn high_readiness_changes_duration_not_the_speed_of_the_clock() {
    let (a,m)=session("a",NOW);let acts=[a];let ml=[m];
    let initial=calc(&acts,&ml,Some(100.0),NOW);
    assert_eq!(initial["leftH"],12.0);
    let deadline=initial["estimatedUntil"].as_i64().unwrap();
    for elapsed in [1,3600,6*3600,11*3600] {
        let later=calc(&acts,&ml,Some(100.0),NOW+elapsed);
        assert_eq!(later["estimatedUntil"],deadline);
        assert_eq!(later["leftH"].as_f64().unwrap(),((deadline-NOW-elapsed) as f64/3600.0).ceil());
    }
    assert_eq!(calc(&acts,&ml,Some(100.0),NOW+13*3600)["leftH"],0.0);
}
#[test] fn low_readiness_also_produces_one_fixed_deadline() {
    let (a,m)=session("a",NOW);let acts=[a];let ml=[m];
    let initial=calc(&acts,&ml,Some(20.0),NOW);let deadline=initial["estimatedUntil"].as_i64().unwrap();
    assert_eq!(initial["leftH"],23.0);
    assert_eq!(calc(&acts,&ml,Some(20.0),NOW+20*3600)["estimatedUntil"],deadline);
    assert_eq!(calc(&acts,&ml,Some(20.0),deadline)["status"],"clear");
}
#[test] fn several_workouts_share_a_fixed_deadline() {
    let (a,ma)=session("a",NOW);let (b,mb)=session("b",NOW-3600);let acts=[a,b];let ml=[ma,mb];
    let initial=calc(&acts,&ml,Some(50.0),NOW);let deadline=initial["estimatedUntil"].as_i64().unwrap();
    assert_eq!(initial["leftH"],22.0);
    for hours in [1,6,12,17,20] {assert_eq!(calc(&acts,&ml,Some(50.0),NOW+hours*3600)["estimatedUntil"],deadline);}
    assert_eq!(calc(&acts,&ml,Some(50.0),deadline)["leftH"],0.0);
}
#[test] fn expired_session_does_not_move_the_group_anchor() {
    let (a,ma)=session("a",NOW);let (b,mb)=session("b",NOW-3600);let acts=[a,b];let ml=[ma,mb];
    let initial=calc(&acts,&ml,None,NOW);let later=calc(&acts,&ml,None,NOW+18*3600);
    assert_eq!(later["estimatedUntil"],initial["estimatedUntil"]);
    assert!(later["workouts"].as_array().unwrap().iter().all(|w|w["leftH"]==0.0));
    assert!(later["leftH"].as_f64().unwrap()>0.0); // Collective model horizon remains active.
}
#[test] fn input_order_does_not_change_grouping() {
    let (a,ma)=session("a",NOW);let (b,mb)=session("b",NOW-3600);
    let first=calc(&[a,b],&[ma,mb],Some(50.0),NOW);
    let (a,ma)=session("a",NOW);let (b,mb)=session("b",NOW-3600);
    let reverse=calc(&[b,a],&[mb,ma],Some(50.0),NOW);
    assert_eq!(first,reverse);
}
#[test] fn separate_groups_are_not_regrouped_as_the_clock_moves() {
    let (a,ma)=session("a",NOW);let (mut b,mut mb)=session("b",NOW-25*3600);
    b.load=120.0;mb.total=60.0;let acts=[a,b];let ml=[ma,mb];
    let initial=calc(&acts,&ml,None,NOW);let deadline=initial["estimatedUntil"].as_i64().unwrap();
    assert_eq!(initial["activeGroups"],2);
    assert_eq!(calc(&acts,&ml,None,NOW+18*3600)["estimatedUntil"],deadline);
}
#[test] fn no_history_is_unknown_not_ready() {
    let r=muscle_svc::recovery_at(&[],&[],&[],&Series::new(),0,None,NOW);
    assert_eq!(r["status"],"unknown");assert!(r["leftH"].is_null());assert!(r["estimatedUntil"].is_null());
    assert!(!r["reasons"].as_array().unwrap().is_empty());
}
#[test] fn established_history_without_active_load_is_clear_not_physiological_clearance() {
    let r=calc(&[],&[],None,NOW);assert_eq!(r["status"],"clear");assert_eq!(r["leftH"],0.0);
}
#[test] fn short_baseline_returns_no_guessed_hours() {
    let (a,m)=session("a",NOW);let today=engine::day_of(NOW,0);
    let ls:Vec<_>=(0..=30).rev().map(|k|LoadDay{date:engine::add_days(&today,-k),ctl:40.0,..Default::default()}).collect();
    let md:Series=ls.iter().map(|l|(l.date.clone(),20.0)).collect();
    let r=muscle_svc::recovery_at(&[a],&[m],&ls,&md,0,None,NOW);
    assert_eq!(r["status"],"unknown");assert!(r["leftH"].is_null());assert!(r["workouts"][0]["hours"].is_null());
}
#[test] fn missing_muscle_data_is_not_silently_zero() {
    let (a,_)=session("a",NOW);let r=calc(&[a],&[],None,NOW);
    assert_eq!(r["status"],"unknown");assert!(r["workouts"][0]["muscleH"].is_null());
}
#[test] fn cardio_history_boundary_is_42_prior_days() {
    let today=engine::day_of(NOW-3600,0);let (_,md)=background();
    for days in [41,42] {
        let ls:Vec<_>=(0..=days).rev().map(|k|LoadDay{date:engine::add_days(&today,-k),ctl:40.0,..Default::default()}).collect();
        let (a,m)=session("a",NOW);let r=muscle_svc::recovery_at(&[a],&[m],&ls,&md,0,None,NOW);
        assert_eq!(r["status"],if days==42{"estimated"}else{"unknown"});
    }
}
#[test] fn muscle_history_boundary_is_28_prior_days() {
    let today=engine::day_of(NOW-3600,0);let (ls,_)=background();
    for days in [27,28] {
        let md:Series=(0..=days).map(|k|(engine::add_days(&today,-k),20.0)).collect();
        let (a,m)=session("a",NOW);let r=muscle_svc::recovery_at(&[a],&[m],&ls,&md,0,None,NOW);
        assert_eq!(r["status"],if days==28{"estimated"}else{"unknown"});
    }
}
#[test] fn a_long_collective_horizon_is_not_cut_off_after_six_days() {
    let mut acts=vec![];let mut ml=vec![];
    for n in 0..6 {
        let (mut a,mut m)=session(&format!("a{n}"),NOW-n*3600);a.load=120.0;m.total=60.0;acts.push(a);ml.push(m);
    }
    let initial=calc(&acts,&ml,Some(20.0),NOW);let later=calc(&acts,&ml,Some(20.0),NOW+145*3600);
    assert!(initial["leftH"].as_f64().unwrap()>144.0);
    assert!(later["leftH"].as_f64().unwrap()>0.0);
    assert_eq!(later["estimatedUntil"],initial["estimatedUntil"]);
}
#[test] fn very_old_unknown_sessions_do_not_keep_today_unknown_forever() {
    let (a,m)=session("old",NOW-120*86400);let (ls,md)=background();
    let ls:Vec<_>=ls.into_iter().filter(|l|l.date>=engine::add_days(&engine::day_of(NOW,0),-125)).collect();
    let r=muscle_svc::recovery_at(&[a],&[m],&ls,&md,0,None,NOW);
    assert_eq!(r["status"],"clear");assert_eq!(r["leftH"],0.0);
}
#[test] fn zero_load_is_not_a_phantom_three_hour_workout() {
    let (mut a,mut m)=session("zero",NOW);a.load=0.0;m.total=0.0;
    let r=calc(&[a],&[m],None,NOW);assert_eq!(r["status"],"clear");assert_eq!(r["workouts"][0]["hours"],0.0);
}
#[test] fn zero_load_does_not_shift_other_groups() {
    let (a,m)=session("a",NOW-3600);let initial=calc(&[a],&[m],None,NOW);
    let (a,m)=session("a",NOW-3600);let (mut zero,mut zm)=session("zero",NOW);zero.load=0.0;zm.total=0.0;
    let combined=calc(&[a,zero],&[m,zm],None,NOW);assert_eq!(combined["estimatedUntil"],initial["estimatedUntil"]);
}
#[test] fn future_and_ongoing_workouts_are_not_counted_as_completed() {
    for offset in [1800,86400] {
        let (a,m)=session("future",NOW+offset);let r=calc(&[a],&[m],None,NOW);
        assert_eq!(r["status"],"unknown");assert!(r["leftH"].is_null());
        assert_eq!(r["ignoredWorkouts"],1);assert!(r["workouts"].as_array().unwrap().is_empty());
    }
}
#[test] fn invalid_numeric_load_is_unknown_not_a_nan_or_zero() {
    for load in [-1.0,f64::NAN,f64::INFINITY] {
        let (mut a,m)=session("bad",NOW);a.load=load;let r=calc(&[a],&[m],None,NOW);
        assert_eq!(r["status"],"unknown");assert!(r["leftH"].is_null());
    }
}
#[test] fn positive_remaining_seconds_do_not_round_to_clear() {
    let (a,m)=session("a",NOW);let acts=[a];let ml=[m];
    let first=calc(&acts,&ml,None,NOW);let deadline=first["estimatedUntil"].as_i64().unwrap();
    assert_eq!(calc(&acts,&ml,None,deadline-1)["leftH"],1.0);
    assert_eq!(calc(&acts,&ml,None,deadline)["leftH"],0.0);
}
#[test] fn incomplete_readiness_cannot_shorten_the_clock() {
    let empty=Series::new();let today=engine::day_of(NOW,0);
    let mut hrv:Series=(0..=30).map(|k|(engine::add_days(&today,-k),50.0)).collect();hrv.insert(today.clone(),70.0);
    let cuts=Default::default();let h=daily::HealthSeries{hrv:&hrv,rhr:&empty,sleep_s:&empty,resp:&empty,breath:&empty,alcohol:&empty,cuts:&cuts};
    let score=daily::readiness(&today,&h,None).score;assert_eq!(score,Some(100.0));
    let trusted=if daily::intensity_limits(&today,&h).is_empty(){score}else{None};
    let (a,m)=session("a",NOW);let r=calc(&[a],&[m],trusted,NOW);
    assert_eq!(r["factor"],1.0);assert_eq!(r["readinessApplied"],false);assert_eq!(r["leftH"],17.0);
}
#[test] fn neutral_factor_and_base_coefficients_are_preserved() {
    let (a,m)=session("a",NOW);assert_eq!(muscle_svc::recovery_hours(&a,Some(&m),40.0,20.0),(17.0,17.0,17.0));
    let r=calc(&[a],&[m],Some(50.0),NOW);assert_eq!(r["factor"],1.0);assert_eq!(r["leftH"],17.0);
}
#[test] fn invalid_readiness_is_ignored() {
    for score in [f64::NAN,f64::INFINITY,-1.0,101.0] {
        let (a,m)=session("a",NOW);let r=calc(&[a],&[m],Some(score),NOW);assert_eq!(r["factor"],1.0);
    }
}
#[test] fn stored_estimates_have_revision_and_legacy_guesses_are_hidden() {
    let c=Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE muscle_cache(activity_id TEXT PRIMARY KEY,data TEXT);").unwrap();
    c.execute("INSERT INTO muscle_cache VALUES('a',?1)",[json!({"t":20,"g":[],"f":20,"rec":{"hours":45,"heartH":45,"muscleH":45}}).to_string()]).unwrap();
    assert!(muscle_svc::for_activity(&c,"a")["rec"].is_null());
    let (a,m)=session("a",NOW);let r=calc(&[a],&[m],None,NOW);muscle_svc::store_recovery(&c,&r);
    let saved=muscle_svc::for_activity(&c,"a");assert_eq!(saved["rec"]["model"],"recovery2");assert_eq!(saved["rec"]["hours"],17.0);
}
#[test] fn unknown_estimate_overwrites_an_old_guess_without_losing_muscle_data() {
    let c=Connection::open_in_memory().unwrap();c.execute_batch("CREATE TABLE muscle_cache(activity_id TEXT PRIMARY KEY,data TEXT);").unwrap();
    c.execute("INSERT INTO muscle_cache VALUES('a',?1)",[json!({"t":20,"g":[],"f":20,"rec":{"hours":45}}).to_string()]).unwrap();
    let (a,m)=session("a",NOW);
    let unknown=muscle_svc::recovery_at(&[a],&[m],&[],&Series::new(),0,None,NOW);
    muscle_svc::store_recovery(&c,&unknown);let v=muscle_svc::for_activity(&c,"a");
    assert_eq!(v["total"],20.0);assert_eq!(v["rec"]["status"],"unknown");assert!(v["rec"]["hours"].is_null());
}
