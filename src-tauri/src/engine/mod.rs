//! Kouros calculation engine. Pure std (no crates) so every formula can be
//! compiled and checked against real data outside the app.
//!
//! Dates are local calendar days as `YYYY-MM-DD` strings; a `Series` maps a day to one value.

pub mod daily;
pub mod training;

use std::collections::BTreeMap;

pub type Series = BTreeMap<String, f64>;

/// The athlete's settings that formulas depend on.
#[derive(Clone, Debug)]
pub struct Profile {
    pub hr_max: f64,
    /// The athlete's name (for the coach texts); empty until the profile is filled in.
    pub name: Option<String>,
    /// The profile was filled in by the athlete (first run asks for it).
    pub filled: bool,
    /// Fixed resting HR; `None` = take the 30-day median from Apple Health.
    pub hr_rest: Option<f64>,
    /// Zone edges, bpm: [Z1 low, Z1/Z2, Z2/Z3, Z3/Z4, Z4/Z5, Z5 high].
    pub zones: [f64; 6],
    pub male: bool,
    pub height_cm: f64,
    pub birth_year: i64,
    /// Local time offset from UTC, seconds (taken from the computer's clock).
    pub tz_offset_s: i64,
    /// Date of the lab test the zones come from.
    pub zones_date: Option<String>,
    /// Threshold (anaerobic) pace, seconds per km.
    pub threshold_pace_s: Option<f64>,
    /// Effective protein, g per kg: the manual value if set, otherwise by goal.
    pub protein_g_per_kg: f64,
    pub protein_manual: Option<f64>,
    /// "maintain" | "lose" | "gain".
    pub goal: String,
    /// Planned change, kg per week (for lose / gain).
    pub goal_rate: f64,
    pub target_weight: Option<f64>,
    /// How much the week may exceed your usual week before it counts as a risky jump (1.2 / 1.3 / 1.4).
    pub week_cap: f64,
    /// Pace zone edges, seconds per km, slow to fast: [Z1 slow, Z1/Z2, Z2/Z3, Z3/Z4, Z4/Z5, Z5 fast].
    pub pace_zones: [f64; 6],
}

impl Default for Profile {
    /// Neutral starting values until the athlete fills in the profile: max HR by the Tanaka formula
    /// for a 35-year-old, zones as % of it, threshold pace 5:30 /km.
    fn default() -> Self {
        Profile {
            hr_max: 184.0,
            name: None,
            filled: false,
            hr_rest: None,
            zones: [110.0, 129.0, 147.0, 160.0, 171.0, 184.0],
            male: true,
            height_cm: 178.0,
            birth_year: 1990,
            tz_offset_s: 0,
            zones_date: None,
            threshold_pace_s: None,
            protein_g_per_kg: 1.8,
            protein_manual: None,
            goal: "maintain".into(),
            goal_rate: 0.5,
            target_weight: None,
            week_cap: 1.3,
            pace_zones: [573.0, 457.0, 383.0, 354.0, 331.0, 312.0],
        }
    }
}

/// Zone edges from max HR (60 / 70 / 80 / 87 / 93 / 100 %).
pub fn zones_from_max(hr_max: f64) -> [f64; 6] {
    let k = [0.60, 0.70, 0.80, 0.87, 0.93, 1.0];
    let mut z = [0.0; 6];
    for i in 0..6 {
        z[i] = (hr_max * k[i]).round();
    }
    z
}

/// Pace zones (s/km, slow → fast) from the threshold pace.
pub fn pace_from_threshold(t: f64) -> [f64; 6] {
    let k = [1.736, 1.384, 1.161, 1.072, 1.003, 0.945];
    let mut z = [0.0; 6];
    for i in 0..6 {
        z[i] = (t * k[i]).round();
    }
    z
}

/// Protein by goal: 2.0 g/kg while losing weight (keeps muscle on a deficit), 1.8 otherwise.
pub fn protein_for_goal(goal: &str) -> f64 {
    if goal == "lose" { 2.0 } else { 1.8 }
}

// ---------- small helpers shared by the modules ----------

pub fn day_of(unix: i64, tz: i64) -> String {
    crate::parse::time::ymd(unix, tz)
}

pub fn day_num(d: &str) -> Option<i64> {
    let y = d.get(0..4)?.parse().ok()?;
    let m = d.get(5..7)?.parse().ok()?;
    let dd = d.get(8..10)?.parse().ok()?;
    Some(crate::parse::time::days_from_civil(y, m, dd))
}

pub fn day_str(n: i64) -> String {
    let (y, m, d) = crate::parse::time::civil_from_days(n);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn add_days(d: &str, k: i64) -> String {
    day_str(day_num(d).unwrap_or(0) + k)
}

/// Values in [from, to) days relative to `date` (e.g. -30..0 = the 30 days before).
pub fn window(s: &Series, date: &str, from: i64, to: i64) -> Vec<f64> {
    let a = add_days(date, from);
    let b = add_days(date, to);
    s.range(a..b).map(|(_, v)| *v).collect()
}

pub fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

pub fn sd(v: &[f64]) -> Option<f64> {
    let m = mean(v)?;
    if v.len() < 2 {
        return None;
    }
    Some((v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt())
}

pub fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    Some(if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 })
}

pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}

/// Latest value on or before `date` within `max_age` days.
pub fn latest(s: &Series, date: &str, max_age: i64) -> Option<(String, f64)> {
    let lo = add_days(date, -max_age);
    let hi = add_days(date, 1);
    s.range(lo..hi).next_back().map(|(d, v)| (d.clone(), *v))
}
