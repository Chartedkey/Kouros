//! Training load (TRIMP → ATL/CTL/TSB, ACWR), heart-rate zones, intensity
//! distribution, best efforts and race predictions.

use super::{add_days, clamp, day_num, day_str, Profile, Series};
use std::collections::BTreeMap;

/// What one workout gives the engine.
#[derive(Clone, Debug, Default)]
pub struct ActInput {
    pub id: String,
    pub sport: String,
    pub start_utc: i64,
    pub duration_s: f64,
    pub distance_m: Option<f64>,
    pub avg_hr: Option<f64>,
    /// (seconds from start, bpm)
    pub hr: Vec<(i64, f64)>,
    /// (seconds from start, metres from start)
    pub dist: Vec<(i64, f64)>,
    /// How hard it felt, 1–10 (session RPE), if the athlete rated it.
    pub rpe: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct ActMetrics {
    pub id: String,
    /// Banister TRIMP (or an estimate when there is no heart rate).
    pub load: f64,
    /// "hr" = pulse curve, "avg_hr" = average pulse, "rpe" = duration × felt effort, "estimate" = by sport and time.
    pub method: &'static str,
    /// Seconds below Z1, in Z1..Z5.
    pub zone_s: [f64; 6],
    /// (distance m, best time s) for runs.
    pub best: Vec<(f64, f64)>,
}

/// Session-RPE (minutes × 1–10) scaled to TRIMP: 60 min at "5" ≈ an hour of easy running.
pub const RPE_SCALE: f64 = 0.45;

pub fn is_rpe_sport(sport: &str) -> bool {
    matches!(sport, "strength" | "workout" | "yoga")
}

/// Distances searched for best efforts.
pub const EFFORTS: [f64; 5] = [1000.0, 5000.0, 10000.0, 21097.5, 42195.0];

fn hrr(hr: f64, rest: f64, max: f64) -> f64 {
    clamp((hr - rest) / (max - rest).max(1.0), 0.0, 1.0)
}

/// Banister TRIMP per minute at a heart-rate reserve fraction.
fn trimp_per_min(x: f64, male: bool) -> f64 {
    let b = if male { 1.92 } else { 1.67 };
    x * 0.64 * (b * x).exp()
}

/// Load per minute when the workout has no heart rate at all.
fn estimate_per_min(sport: &str) -> f64 {
    match sport {
        "run" => 1.4,
        "ride" | "ride_virtual" => 1.0,
        "strength" => 0.9,
        "hike" => 0.8,
        "walk" => 0.5,
        "swim" => 1.2,
        "yoga" => 0.35,
        "workout" => 0.5,
        _ => 0.6,
    }
}

pub fn zone_index(p: &Profile, hr: f64) -> usize {
    let z = &p.zones;
    if hr < z[0] {
        0
    } else if hr < z[1] {
        1
    } else if hr < z[2] {
        2
    } else if hr < z[3] {
        3
    } else if hr < z[4] {
        4
    } else {
        5
    }
}

pub fn activity_metrics(p: &Profile, rest: f64, a: &ActInput) -> ActMetrics {
    let mut m = ActMetrics { id: a.id.clone(), ..Default::default() };
    let usable = a.hr.len() >= 10;
    if usable {
        let mut trimp = 0.0;
        for w in a.hr.windows(2) {
            let dt = (w[1].0 - w[0].0) as f64;
            if !(0.0..=30.0).contains(&dt) {
                continue; // pauses don't count
            }
            let hr = w[0].1;
            trimp += trimp_per_min(hrr(hr, rest, p.hr_max), p.male) * dt / 60.0;
            m.zone_s[zone_index(p, hr)] += dt;
        }
        m.load = trimp;
        m.method = "hr";
    } else if let Some(avg) = a.avg_hr.filter(|h| *h > 40.0) {
        m.load = trimp_per_min(hrr(avg, rest, p.hr_max), p.male) * a.duration_s / 60.0;
        m.zone_s[zone_index(p, avg)] = a.duration_s;
        m.method = "avg_hr";
    } else {
        m.load = estimate_per_min(&a.sport) * a.duration_s / 60.0;
        m.method = "estimate";
    }
    // Strength and physio barely raise the pulse, so a felt-effort rating describes them better.
    if let Some(r) = a.rpe.filter(|r| *r >= 1.0 && is_rpe_sport(&a.sport)) {
        m.load = a.duration_s / 60.0 * r * RPE_SCALE;
        m.method = "rpe";
    }
    if a.sport == "run" && a.dist.len() > 10 {
        m.best = best_efforts(&a.dist);
    }
    m
}

/// Fastest time over each standard distance inside one run (two-pointer scan).
pub fn best_efforts(dist: &[(i64, f64)]) -> Vec<(f64, f64)> {
    let total = dist.last().map(|d| d.1).unwrap_or(0.0) - dist.first().map(|d| d.1).unwrap_or(0.0);
    let mut out = Vec::new();
    for &d in EFFORTS.iter() {
        if total < d {
            break;
        }
        let mut best = f64::INFINITY;
        let mut i = 0;
        for j in 0..dist.len() {
            while i < j && dist[j].1 - dist[i + 1].1 >= d {
                i += 1;
            }
            let covered = dist[j].1 - dist[i].1;
            if covered >= d {
                // Interpolate to the exact distance within the last step.
                let t = (dist[j].0 - dist[i].0) as f64;
                let over = covered - d;
                let step = dist[j].1 - dist[j - 1].1;
                let step_t = (dist[j].0 - dist[j - 1].0) as f64;
                let adj = if step > 0.0 { over / step * step_t } else { 0.0 };
                best = best.min(t - adj);
            }
        }
        if best.is_finite() && best > d / 8.0 {
            // Faster than 8 m/s for the whole distance is a GPS glitch, not a record.
            out.push((d, best));
        }
    }
    out
}

/// Daily totals of load, keyed by local day.
pub fn daily_load(acts: &[(String, f64)]) -> Series {
    let mut s = Series::new();
    for (d, l) in acts {
        *s.entry(d.clone()).or_insert(0.0) += *l;
    }
    s
}

#[derive(Clone, Debug, Default)]
pub struct LoadDay {
    pub date: String,
    pub load: f64,
    /// Fatigue: 7-day exponentially weighted load.
    pub atl: f64,
    /// Fitness: 42-day exponentially weighted load.
    pub ctl: f64,
    /// Form at the start of the day: yesterday's CTL − ATL.
    pub tsb: f64,
    /// Acute:chronic ratio — 7-day sum vs average week of the last 28 days.
    pub acwr: Option<f64>,
    pub week_sum: f64,
}

pub fn load_series(daily: &Series, to: &str) -> Vec<LoadDay> {
    let Some(first) = daily.keys().next().and_then(|d| day_num(d)) else {
        return Vec::new();
    };
    let last = day_num(to).unwrap_or(first);
    let ka = 1.0 - (-1.0f64 / 7.0).exp();
    let kc = 1.0 - (-1.0f64 / 42.0).exp();
    let (mut atl, mut ctl) = (0.0, 0.0);
    let mut loads: Vec<f64> = Vec::new();
    let mut out = Vec::new();
    for n in first..=last {
        let date = day_str(n);
        let load = *daily.get(&date).unwrap_or(&0.0);
        let tsb = ctl - atl;
        atl += (load - atl) * ka;
        ctl += (load - ctl) * kc;
        loads.push(load);
        let k = loads.len();
        let week: f64 = loads[k.saturating_sub(7)..].iter().sum();
        let month: f64 = loads[k.saturating_sub(28)..].iter().sum();
        let acwr = if k >= 28 && month > 0.0 { Some(week / (month / 4.0)) } else { None };
        out.push(LoadDay { date, load, atl, ctl, tsb, acwr, week_sum: week });
    }
    out
}

/// Plain-language load state for the home screen.
pub fn load_state(acwr: Option<f64>) -> (&'static str, &'static str) {
    match acwr {
        None => ("unknown", "Мало истории, чтобы оценить"),
        Some(r) if r < 0.8 => ("low", "Ниже обычного — можно добавлять"),
        Some(r) if r <= 1.3 => ("optimal", "В норме"),
        Some(r) if r <= 1.5 => ("high", "Выше обычного — следи за восстановлением"),
        Some(_) => ("very_high", "Резкий рост — риск перегруза"),
    }
}

/// Share of time in low (Z1–Z2 and below), moderate (Z3) and high (Z4–Z5) intensity.
pub fn intensity_split(zone_s: &[[f64; 6]]) -> Option<(f64, f64, f64, f64)> {
    let mut low = 0.0;
    let mut mid = 0.0;
    let mut high = 0.0;
    for z in zone_s {
        low += z[0] + z[1] + z[2];
        mid += z[3];
        high += z[4] + z[5];
    }
    let total = low + mid + high;
    if total < 600.0 {
        return None;
    }
    Some((low / total, mid / total, high / total, total))
}

/// Riegel: T2 = T1 · (D2 / D1)^1.06.
pub fn riegel(t1: f64, d1: f64, d2: f64) -> f64 {
    t1 * (d2 / d1).powf(1.06)
}

#[derive(Clone, Debug)]
pub struct Prediction {
    pub distance_m: f64,
    pub time_s: f64,
    pub from_distance_m: f64,
    pub from_time_s: f64,
    pub from_date: String,
}

/// Predictions from the strongest recent effort (best Riegel-equivalent 10 km).
pub fn predictions(efforts: &[(String, f64, f64)]) -> Vec<Prediction> {
    // efforts: (date, distance, time) — only 5 km and longer are trustworthy.
    let mut best: Option<&(String, f64, f64)> = None;
    let mut best_eq = f64::INFINITY;
    for e in efforts.iter().filter(|e| e.1 >= 5000.0) {
        let eq = riegel(e.2, e.1, 10000.0);
        if eq < best_eq {
            best_eq = eq;
            best = Some(e);
        }
    }
    let Some((date, d, t)) = best else { return Vec::new() };
    [5000.0, 10000.0, 21097.5, 42195.0]
        .iter()
        .map(|&d2| Prediction {
            distance_m: d2,
            time_s: riegel(*t, *d, d2),
            from_distance_m: *d,
            from_time_s: *t,
            from_date: date.clone(),
        })
        .collect()
}

/// Best time per distance across all runs: distance → (date, seconds, activity id).
pub fn records(all: &[(String, String, Vec<(f64, f64)>)]) -> BTreeMap<u64, (String, f64, String)> {
    let mut out: BTreeMap<u64, (String, f64, String)> = BTreeMap::new();
    for (id, date, best) in all {
        for (d, t) in best {
            let k = *d as u64;
            if out.get(&k).map(|x| *t < x.1).unwrap_or(true) {
                out.insert(k, (date.clone(), *t, id.clone()));
            }
        }
    }
    out
}

/// Days since a date (for the zone retest reminder).
pub fn days_since(date: &str, today: &str) -> Option<i64> {
    Some(day_num(today)? - day_num(date)?)
}

pub fn week_start(date: &str) -> String {
    // Monday of the week: 1970-01-01 was a Thursday.
    let n = day_num(date).unwrap_or(0);
    let wd = (n + 3).rem_euclid(7); // 0 = Monday
    add_days(&day_str(n), -wd)
}
