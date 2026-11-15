//! Diagnostic report for the current model. Historical audit evidence is archived separately.
//! Acceptance criteria for fixed deadlines are in src/recovery_tests.rs.
use kouros_calculation_tests::{engine::{self, training::LoadDay, Series}, engine_svc::Cached, muscle_svc::{self, MLoad}, store};
use serde_json::{json, Value};
use std::collections::HashMap;

fn activity(id: &str, end: i64, minutes: f64, load: f64, sport: &str) -> Cached {
    Cached { id: id.into(), sport: sport.into(), start_utc: end - (minutes * 60.0) as i64,
        duration_s: minutes * 60.0, distance_m: Some(minutes * 100.0), calories: None,
        load, method: "hr".into(), zone_s: [0.0; 6], best: vec![] }
}
fn muscle(a: &Cached, end: i64, total: f64, group: usize) -> MLoad {
    let mut groups = [0.0; 12]; groups[group] = total;
    MLoad { id: a.id.clone(), sport: a.sport.clone(), start: a.start_utc, end, total,
        groups, up: 0.0, down: 0.0, bad: false, free: 0.0 }
}
fn history(now: i64, ctl: f64, usual: f64) -> (Vec<LoadDay>, Series) {
    let today = engine::day_of(now, 0);
    let dates = (-200..=1).map(|k| engine::add_days(&today, k)).collect::<Vec<_>>();
    (dates.iter().map(|d| LoadDay { date: d.clone(), ctl, ..Default::default() }).collect(),
        dates.iter().map(|d| (d.clone(), usual)).collect())
}
fn hours(a: &Cached, m: &MLoad, ctl: f64, usual: f64) -> Value {
    let (total, heart, muscles) = muscle_svc::recovery_hours(a, Some(m), ctl, usual);
    json!({"hours":total,"heart":heart,"muscles":muscles})
}
fn main() {
    let now = store::now();
    let (ls, mdaily) = history(now, 40.0, 20.0);
    let a = activity("a", now, 60.0, 40.0, "run");
    let m = muscle(&a, now, 20.0, 0);
    let mut results = serde_json::Map::new();
    let h = hours(&a, &m, 40.0, 20.0);
    results.insert("normal_single".into(),h);

    let mut trajectory = vec![];
    for ready in [None, Some(20.0), Some(50.0), Some(100.0)] {
        for elapsed in 0..=24 {
            let end = now - elapsed * 3600;
            let a = activity("a", end, 60.0, 40.0, "run"); let m = muscle(&a, end, 20.0, 0);
            let r = muscle_svc::recovery(&[a], &[m], &ls, &mdaily, 0, ready);
            trajectory.push(json!({"readiness":ready,"elapsed_h":elapsed,"left_h":r["leftH"],
                "factor":r["factor"],"status":r["status"],"estimated_finish_from_original_end_h":r["estimatedUntil"].as_i64().map(|t|(t-end) as f64/3600.0)}));
        }
    }

    results.insert("clock_trajectory".into(),json!(trajectory));

    let mut zero = activity("zero",now,60.0,0.0,"run"); let zm=muscle(&zero,now,0.0,0);

    results.insert("zero_load".into(),hours(&zero,&zm,40.0,20.0));
    let empty = muscle_svc::recovery(&[], &[], &[], &Series::new(), 0, None);
     results.insert("no_workouts".into(),empty);

    let mut cold = vec![];
    for ctl in [0.0,1.0,5.0,40.0] { for load in [40.0,50.0,150.0] {
        zero.load=load; let m=muscle(&zero,now,0.0,0);
        cold.push(json!({"ctl":ctl,"load":load,"result":hours(&zero,&m,ctl,20.0)}));
    }}
    results.insert("cold_start_saturation".into(),json!(cold));

    let mut segmentation=vec![];
    for count in [1,2,3,6] {
        let each_minutes=60.0/count as f64; let mut acts=vec![];let mut muscles=vec![];
        for n in 0..count {
            let end=now-((count-1-n) as f64*each_minutes*60.0) as i64;
            let a=activity(&format!("segment{n}"),end,each_minutes,40.0/count as f64,"run");
            muscles.push(muscle(&a,end,20.0/count as f64,0));acts.push(a);
        }
        let r=muscle_svc::recovery(&acts,&muscles,&ls,&mdaily,0,Some(50.0));
        segmentation.push(json!({"records":count,"total_load":40,"total_minutes":60,"left_h":r["leftH"]}));
    }

    results.insert("one_workout_split_into_consecutive_records".into(),json!(segmentation));

    let mut multiple=vec![];
    for count in [1,2,3] {
        for elapsed in [0,6,12,17] {
            let mut acts=vec![];let mut muscles=vec![];
            for n in 0..count {
                let end=now-(elapsed+n)*3600;
                let a=activity(&format!("session{n}"),end,60.0,40.0,"run");
                muscles.push(muscle(&a,end,20.0,n as usize));acts.push(a);
            }
            let r=muscle_svc::recovery(&acts,&muscles,&ls,&mdaily,0,Some(50.0));
            multiple.push(json!({"sessions":count,"elapsed_h":elapsed,"left_h":r["leftH"]}));
        }
    }
    results.insert("multiple_session_countdown".into(),json!(multiple));

    let mut timing=vec![];
    for age in [86399,86400,86401] {
        let a=activity("new",now,60.0,120.0,"run");let b=activity("old",now-age,60.0,100.0,"run");
        let ma=muscle(&a,now,60.0,0);let mb=muscle(&b,now-age,40.0,0);
        let r=muscle_svc::recovery(&[a,b],&[ma,mb],&ls,&mdaily,0,Some(50.0));
        timing.push(json!({"separation_s":age,"left_h":r["leftH"]}));
    }

    results.insert("twenty_four_hour_grouping_boundary".into(),json!(timing));

    let mut labels=vec![];
    for sport in ["walk","hike","run"] {
        let a=activity("label",now,120.0,120.0,sport);let m=muscle(&a,now,60.0,0);
        labels.push(json!({"sport":sport,"result":hours(&a,&m,40.0,20.0)}));
    }
    results.insert("sport_label_only".into(),json!(labels));

    let mut paired=vec![];
    for total in [21.4,21.5] {
        let a=activity("threshold",now,60.0,60.0,"run");let m=muscle(&a,now,total,0);
        paired.push(json!({"muscle_load":total,"result":hours(&a,&m,40.0,20.0)}));
    }

    results.insert("four_hour_bonus_boundary".into(),json!(paired));

    let mut distributions=vec![];
    for group in [0,7] {
        let a=activity("groups",now,60.0,40.0,"strength");let m=muscle(&a,now,20.0,group);
        distributions.push(json!({"dominant_group":group,"result":hours(&a,&m,40.0,20.0)}));
    }

    results.insert("different_muscle_groups_same_total".into(),json!(distributions));

    let mut origin=vec![];
    for method in ["hr","avg_hr","rpe","estimate"] {
        let mut a=activity("method",now,60.0,40.0,"run");a.method=method.into();a.zone_s[4]=1200.0;
        let m=muscle(&a,now,20.0,0);
        origin.push(json!({"method":method,"result":hours(&a,&m,40.0,20.0)}));
    }
    results.insert("load_confidence_is_not_used".into(),json!(origin));

    let mut futures=vec![];
    for offset in [-1800,0,3600,86400] {
        let end=now+offset;let a=activity("future",end,60.0,40.0,"run");let m=muscle(&a,end,20.0,0);
        let r=muscle_svc::recovery(&[a],&[m],&ls,&mdaily,0,None);
        futures.push(json!({"workout_end_offset_s":offset,"left_h":r["leftH"]}));
    }
    results.insert("future_or_ongoing_records".into(),json!(futures));

    let mut rounding=vec![];
    for age in [2.49,2.51,2.99,3.0] {
        let end=now-(age*3600.0) as i64;
        let a=activity("rounding",end,1.0,0.0,"run");let m=muscle(&a,end,0.0,0);
        let r=muscle_svc::recovery(&[a],&[m],&ls,&mdaily,0,None);
        rounding.push(json!({"elapsed_h":age,"left_h":r["leftH"]}));
    }
    results.insert("rounding_to_ready".into(),json!(rounding));

    let mut muscle_maps=vec![];
    for age in [0,17,24,36,48] {
        let end=now-age*3600;let a=activity("map",end,60.0,40.0,"strength");let m=muscle(&a,end,20.0,0);
        let mut ml:Vec<MLoad>=(1..=28).map(|day|{
            let end=now-day*86400;let a=activity(&format!("past{day}"),end,60.0,40.0,"strength");muscle(&a,end,20.0,0)
        }).collect();ml.push(m);
        let map=muscle_svc::map_state(&ml,&HashMap::new());
        let r=muscle_svc::recovery(&[a],&ml,&ls,&mdaily,0,None);
        muscle_maps.push(json!({"elapsed_h":age,"left_h":r["leftH"],"map_state":map[0]["state"],"map_ratio":map[0]["ratio"]}));
    }
    results.insert("countdown_vs_muscle_map".into(),json!(muscle_maps));
    println!("{}",serde_json::to_string_pretty(&json!({"schema":1,"observed_at_unix":now,"synthetic_data_only":true,"cases":results})).unwrap());
}
