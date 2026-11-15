use crate::engine::{add_days, daily::{self, Gates, HealthSeries}, Profile, Series};
use std::collections::BTreeMap;

const TODAY: &str = "2026-11-15";

struct Data {
    hrv: Series,
    rhr: Series,
    sleep: Series,
    resp: Series,
    empty: Series,
    cuts: BTreeMap<String, String>,
}

impl Data {
    fn healthy(nights: i64) -> Self {
        let history = |value| (1..=nights).map(|k| (add_days(TODAY, -k), value)).collect::<Series>();
        let mut d = Self { hrv: history(50.0), rhr: history(55.0), sleep: history(8.0 * 3600.0),
            resp: history(13.0), empty: Series::new(), cuts: BTreeMap::new() };
        d.hrv.insert(TODAY.into(), 70.0);
        d.rhr.insert(TODAY.into(), 45.0);
        d.sleep.insert(TODAY.into(), 8.0 * 3600.0);
        d.resp.insert(TODAY.into(), 13.0);
        d
    }
    fn series(&self) -> HealthSeries<'_> {
        HealthSeries { hrv: &self.hrv, rhr: &self.rhr, sleep_s: &self.sleep,
            resp: &self.resp, breath: &self.empty, alcohol: &self.empty, cuts: &self.cuts }
    }
    fn gates(&self) -> Gates {
        Gates { week_heavy: false, quality_yesterday: false, body_warning: false,
            feel: 0, feel_reason: None, has_survey: false, drank: false,
            intensity_limits: daily::intensity_limits(TODAY, &self.series()) }
    }
    fn plan(&self, score: f64) -> daily::DayPlan {
        daily::plan_day(Some(score), &Profile::default().zones, &self.gates())
    }
}

#[test] fn audit_hrv_only_keeps_provisional_score_but_cannot_recommend_intervals() {
    let mut d = Data::healthy(30);
    d.rhr.clear(); d.sleep.clear(); d.resp.clear();
    let r = daily::readiness(TODAY, &d.series(), None);
    assert_eq!(r.score, Some(100.0));
    let p = d.plan(r.score.unwrap());
    assert_eq!(p.body_tier, 6); assert_eq!(p.tier, 3);
    assert!(p.reasons.iter().any(|r| r.contains("пульса покоя")));
    assert!(p.reasons.iter().any(|r| r.contains("сне")));
}

#[test] fn audit_three_hours_of_sleep_cannot_be_compensated_by_high_hrv() {
    let mut d = Data::healthy(30); d.sleep.insert(TODAY.into(), 3.0 * 3600.0);
    let r = daily::readiness(TODAY, &d.series(), Some(20.0));
    assert_eq!(r.score, Some(70.0)); assert_eq!(r.level, "green");
    let p = d.plan(r.score.unwrap()); assert_eq!(p.tier, 3);
    assert!(p.reasons.iter().any(|r| r.contains("короче 6 часов")));
    assert_eq!(daily::readiness(TODAY, &d.series(), Some(20.0)).score, r.score);
}

#[test] fn missing_either_current_core_metric_blocks_intensity() {
    for hrv in [true, false] {
        let mut d = Data::healthy(30);
        if hrv { d.hrv.remove(TODAY); } else { d.rhr.remove(TODAY); }
        assert_eq!(d.plan(100.0).tier, 3);
        assert_eq!(d.gates().intensity_limits.len(), 1);
    }
}

#[test] fn yesterday_and_future_readings_cannot_substitute_for_today() {
    let mut d = Data::healthy(30);
    let tomorrow = add_days(TODAY, 1);
    d.hrv.remove(TODAY); d.rhr.remove(TODAY); d.sleep.remove(TODAY);
    d.hrv.insert(tomorrow.clone(), 70.0); d.rhr.insert(tomorrow.clone(), 45.0);
    d.sleep.insert(tomorrow, 8.0 * 3600.0);
    let g = d.gates(); assert_eq!(g.intensity_limits.len(), 3);
    assert_eq!(d.plan(100.0).tier, 3);
}

#[test] fn missing_sleep_blocks_intensity_even_with_both_core_metrics() {
    let mut d = Data::healthy(30); d.sleep.remove(TODAY);
    let p = d.plan(100.0); assert_eq!(p.tier, 3);
    assert_eq!(p.reasons.len(), 1); assert!(p.reasons[0].contains("сне"));
}

#[test] fn sleep_cutoff_is_explicit_and_does_not_mean_six_hours_is_adequate_sleep() {
    for (seconds, allowed) in [(21599.0, false), (21600.0, true), (28800.0, true)] {
        let mut d = Data::healthy(30); d.sleep.insert(TODAY.into(), seconds);
        assert_eq!(d.gates().intensity_limits.is_empty(), allowed);
        assert_eq!(d.plan(100.0).tier, if allowed { 6 } else { 3 });
    }
}

#[test] fn zero_negative_and_nonfinite_measurements_are_missing_evidence() {
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        for metric in ["hrv", "rhr", "sleep"] {
            let mut d = Data::healthy(30);
            match metric { "hrv" => &mut d.hrv, "rhr" => &mut d.rhr, _ => &mut d.sleep }.insert(TODAY.into(), value);
            assert_eq!(d.plan(100.0).tier, 3, "{metric}: {value}");
        }
    }
}

#[test] fn fourteen_prior_paired_days_are_required_not_four_or_thirteen() {
    for (nights, allowed) in [(4, false), (13, false), (14, true), (30, true)] {
        let d = Data::healthy(nights);
        assert_eq!(d.gates().intensity_limits.is_empty(), allowed);
        assert_eq!(d.plan(100.0).tier, if allowed { 6 } else { 3 });
    }
}

#[test] fn current_day_does_not_fill_the_fourteenth_baseline_day() {
    let d = Data::healthy(13);
    assert!(d.gates().intensity_limits.iter().any(|r| r.contains("13 из 14")));
}

#[test] fn unpaired_histories_do_not_count_as_fourteen_comparable_days() {
    let mut d = Data::healthy(14);
    d.rhr = (15..=28).map(|k| (add_days(TODAY, -k), 55.0)).collect();
    d.rhr.insert(TODAY.into(), 45.0);
    assert!(d.gates().intensity_limits.iter().any(|r| r.contains("0 из 14")));
}

#[test] fn old_and_future_baselines_cannot_fill_the_current_window() {
    let mut d = Data::healthy(0);
    for k in (31..=50).chain(-20..=-1) {
        d.hrv.insert(add_days(TODAY, -k), 50.0); d.rhr.insert(add_days(TODAY, -k), 55.0);
    }
    assert!(d.gates().intensity_limits.iter().any(|r| r.contains("0 из 14")));
}

#[test] fn either_source_switch_restarts_paired_evidence() {
    for group in ["hrv", "rhr"] {
        let mut d = Data::healthy(30); d.cuts.insert(group.into(), add_days(TODAY, -10));
        assert!(d.gates().intensity_limits.iter().any(|r| r.contains("10 из 14")));
        assert_eq!(d.plan(100.0).tier, 3);
    }
}

#[test] fn source_cut_boundary_and_future_cut_match_baseline_semantics() {
    let mut d = Data::healthy(30);
    d.cuts.insert("hrv".into(), add_days(TODAY, -14));
    assert!(d.gates().intensity_limits.is_empty());
    d.cuts.insert("rhr".into(), add_days(TODAY, 1));
    assert!(d.gates().intensity_limits.is_empty());
}

#[test] fn invalid_historical_pairs_do_not_establish_a_baseline() {
    let mut d = Data::healthy(14);
    d.rhr.insert(add_days(TODAY, -1), f64::NAN);
    assert!(d.gates().intensity_limits.iter().any(|r| r.contains("13 из 14")));
}

#[test] fn complete_evidence_preserves_existing_tiers_and_optional_survey() {
    let d = Data::healthy(30);
    for score in [10.0, 30.0, 40.0, 50.0, 60.0, 67.0, 80.0] {
        assert_eq!(d.plan(score).tier, daily::tier_of(score));
    }
    assert!(d.plan(80.0).hint.unwrap().contains("TQR"));
}

#[test] fn evidence_cap_never_raises_a_lower_tier() {
    let mut d = Data::healthy(30); d.sleep.remove(TODAY);
    for score in [10.0, 30.0, 40.0, 50.0, 60.0, 67.0, 100.0] {
        assert_eq!(d.plan(score).tier, daily::tier_of(score).min(3));
    }
}

#[test] fn other_existing_limits_still_apply_and_are_explained() {
    let d = Data::healthy(30);
    let mut gates = d.gates(); gates.week_heavy = true;
    let p = daily::plan_day(Some(90.0), &Profile::default().zones, &gates);
    assert_eq!(p.tier, 4); assert!(p.reasons.iter().any(|r| r.contains("за неделю")));
    gates.intensity_limits.push("нет данных о сне".into());
    let p = daily::plan_day(Some(90.0), &Profile::default().zones, &gates);
    assert_eq!(p.tier, 3); assert!(p.reasons.iter().any(|r| r.contains("за неделю")));
    assert!(p.reasons.iter().any(|r| r.contains("сне")));
}

#[test] fn subjective_fatigue_cannot_be_overridden_by_complete_evidence() {
    let d = Data::healthy(30); let mut gates = d.gates();
    gates.feel = 3; gates.feel_reason = Some("низкий TQR".into());
    let p = daily::plan_day(Some(80.0), &Profile::default().zones, &gates);
    assert_eq!(p.tier, 4); assert!(p.reasons.iter().any(|r| r.contains("TQR")));
}

#[test] fn a_high_evening_score_cannot_bypass_the_same_evidence_cap() {
    let mut d = Data::healthy(30); d.sleep.insert(TODAY.into(), 3.0 * 3600.0);
    for morning_or_evening in [70.0, 85.0, 100.0] { assert_eq!(d.plan(morning_or_evening).tier, 3); }
}

#[test] fn limited_plan_uses_the_calm_tier_range_instead_of_the_high_score_range() {
    let mut d = Data::healthy(30); d.sleep.remove(TODAY);
    let p = d.plan(100.0);
    let expected = daily::tier_factors(3, Some(50.0));
    assert_eq!((p.lo, p.hi), expected);
    assert!(p.hi < daily::tier_factors(6, Some(100.0)).1);
}

#[test] fn no_score_is_not_a_permission_for_intensity_and_reasons_are_retained() {
    let mut d = Data::healthy(0); d.hrv.clear(); d.rhr.clear(); d.sleep.clear();
    let p = daily::plan_day(None, &Profile::default().zones, &d.gates());
    assert_eq!(p.tier, 3); assert!(!p.reasons.is_empty());
}
