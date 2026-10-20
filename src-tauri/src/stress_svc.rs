//! Daily stress (0–100) from the watch and evening readiness.
//!
//! Stress is read only while awake and still: pulse above the night resting pulse and daytime HRV
//! spot checks below your usual daytime level. Strava workouts (plus 20 minutes after them), sleep
//! and minutes with walking are left out, so effort is not counted as stress.

use crate::engine::{add_days, mean, median, window, Profile, Series};
use crate::health_svc::{hypnograms, raw_samples};
use crate::parse::time;
use crate::store;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const BIN: i64 = 900; // 15 minutes

pub struct StressDay {
    pub score: Option<f64>,
    pub hr_part: Option<f64>,
    pub hrv_part: Option<f64>,
    /// When the day started (wake-up) and ended (bedtime or now).
    pub wake: i64,
    pub end: i64,
    /// 15-minute bins: (start, stress 0–100).
    pub bins: Vec<(i64, f64)>,
    /// Minutes in: покой (<25), низкий (25–49), средний (50–74), высокий (≥75).
    pub levels: [f64; 4],
}

fn level_idx(s: f64) -> usize {
    if s < 25.0 {
        0
    } else if s < 50.0 {
        1
    } else if s < 75.0 {
        2
    } else {
        3
    }
}

pub fn level_word(s: f64) -> &'static str {
    ["покой", "низкий", "средний", "высокий"][level_idx(s)]
}

fn day_start(d: &str, tz: i64) -> i64 {
    let g = |a: usize, b: usize| d.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(1);
    time::unix(g(0, 4), g(5, 7), g(8, 10), 0, 0, 0) - tz
}

/// Stress for every day from `from` to `to` (by the date of the morning you woke up).
pub fn days(conn: &Connection, p: &Profile, from: &str, to: &str) -> BTreeMap<String, StressDay> {
    let tz = p.tz_offset_s;
    let today = crate::engine_svc::today(p).min(to.to_string());
    let now = store::now();
    let mut out = BTreeMap::new();
    let hyps = hypnograms(conn, &add_days(from, -31));
    let hyp_by: BTreeMap<String, (i64, i64)> = hyps.iter().map(|h| (h.date.clone(), (h.start, h.end))).collect();
    let asleep = |t: i64| hyps.iter().any(|h| h.asleep(t));
    let hr = raw_samples(conn, "heart_rate_raw", &add_days(from, -1), 30.0, 230.0);
    if hr.is_empty() {
        return out;
    }
    let hrv = raw_samples(conn, "hrv_sdnn_raw", &add_days(from, -31), 5.0, 300.0);
    let steps = raw_samples(conn, "steps_raw", &add_days(from, -1), 0.0, 100000.0);
    let acts: Vec<(i64, i64)> = match conn.prepare("SELECT start_utc, COALESCE(elapsed_s, moving_s, 0) FROM va WHERE start_utc >= ?1") {
        Ok(mut st) => {
            let v: Vec<(i64, i64)> = st
                .query_map(params![day_start(from, tz) - 86400], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)? as i64)))
                .map(|r| r.flatten().collect())
                .unwrap_or_default();
            v
        }
        Err(_) => Vec::new(),
    };
    let in_workout = |t: i64| acts.iter().any(|(s, d)| t >= s - 300 && t <= s + d + 1200);
    let moving = |t: i64| steps.range(t - 300..=t + 60).map(|(_, v)| *v).sum::<f64>() >= 60.0;
    let still = |t: i64| !asleep(t) && !in_workout(t) && !moving(t);
    let (rhr_s, _) = crate::health_svc::rhr_series(conn);
    let rhr_fallback = crate::engine_svc::resting_hr(conn, p);

    let mut d = from.to_string();
    while d <= today {
        let wake = hyp_by.get(&d).map(|x| x.1).unwrap_or_else(|| day_start(&d, tz) + 5 * 3600);
        let next = hyp_by.get(&add_days(&d, 1)).map(|x| x.0).filter(|s| *s > wake);
        let end = next.unwrap_or(wake + 20 * 3600).min(wake + 22 * 3600).min(now);
        if end <= wake {
            d = add_days(&d, 1);
            continue;
        }
        let rhr = median(&window(&rhr_s, &d, -30, 1)).unwrap_or(rhr_fallback);
        let span = (p.hr_max - rhr).max(40.0) * 0.30;
        let hr_stress = |v: f64| ((v - (rhr + 8.0)) / span).clamp(0.0, 1.0) * 100.0;
        let mut bins = Vec::new();
        let mut b = wake - wake.rem_euclid(BIN);
        while b < end {
            let v: Vec<f64> = hr.range(b.max(wake)..(b + BIN).min(end)).filter(|(t, _)| still(**t)).map(|(_, v)| *v).collect();
            if let Some(m) = median(&v) {
                bins.push((b, hr_stress(m).round()));
            }
            b += BIN;
        }
        // Daytime HRV against your usual daytime HRV (the 30 days before), never against the night.
        let base: Vec<f64> = hrv.range(wake - 30 * 86400..wake).filter(|(t, _)| still(**t)).map(|(_, v)| *v).collect();
        let hrv_part = if base.len() >= 20 {
            let n = median(&base).unwrap_or(0.0);
            let today_v: Vec<f64> = hrv
                .range(wake..end)
                .filter(|(t, _)| still(**t))
                .map(|(_, v)| (25.0 + 75.0 * (n - v) / (0.5 * n)).clamp(0.0, 100.0))
                .collect();
            if today_v.len() >= 2 { mean(&today_v) } else { None }
        } else {
            None
        };
        let hr_part = if bins.len() >= 8 { mean(&bins.iter().map(|x| x.1).collect::<Vec<_>>()) } else { None };
        let score = match (hr_part, hrv_part) {
            (Some(a), Some(b)) => Some((0.6 * a + 0.4 * b).round()),
            (Some(a), None) => Some(a.round()),
            _ => None,
        };
        let mut levels = [0.0; 4];
        for (_, s) in &bins {
            levels[level_idx(*s)] += 15.0;
        }
        out.insert(d.clone(), StressDay { score, hr_part, hrv_part, wake, end, bins, levels });
        d = add_days(&d, 1);
    }
    out
}

pub fn day_json(s: &StressDay, tz: i64, norm: Option<f64>) -> Value {
    let clock = |t: i64| {
        let m = (t + tz).rem_euclid(86400) / 60;
        format!("{:02}:{:02}", m / 60, m % 60)
    };
    json!({
        "score": s.score, "level": s.score.map(level_word), "norm": norm,
        "hr": s.hr_part.map(|x| x.round()), "hrv": s.hrv_part.map(|x| x.round()),
        "from": clock(s.wake), "to": clock(s.end),
        "levels": s.levels,
        "bins": s.bins.iter().map(|(t, v)| json!([t, v])).collect::<Vec<_>>(),
    })
}

/// Usual daily stress: median of the 30 days before `d`.
pub fn norm(all: &BTreeMap<String, StressDay>, d: &str) -> Option<f64> {
    let s: Series = all.iter().filter_map(|(k, v)| v.score.map(|x| (k.clone(), x))).collect();
    let v = window(&s, d, -30, 0);
    if v.len() < 4 { None } else { median(&v).map(|x| x.round()) }
}

pub struct EveningInput<'a> {
    pub morning: Option<f64>,
    pub load_done: f64,
    pub usual_day: f64,
    pub target_hi: f64,
    pub kcal: Option<f64>,
    pub kcal_target: f64,
    pub water: Option<f64>,
    pub water_target: f64,
    pub alcohol: Option<f64>,
    pub caffeine: Option<f64>,
    pub stress: Option<&'a StressDay>,
    pub stress_norm: Option<f64>,
    pub tqr: Option<f64>,
}

/// Evening readiness = frozen morning score ± what the day did to it (at most ±15).
pub fn evening(e: &EveningInput) -> Value {
    let Some(m) = e.morning else { return Value::Null };
    let now = store::now();
    let hours = e.stress.map(|s| (now - s.wake) as f64 / 3600.0).unwrap_or(0.0);
    let mut items: Vec<(String, f64)> = Vec::new();
    if e.load_done >= 5.0 {
        let mut pts = -(e.load_done / e.usual_day.max(1.0) * 4.0).min(8.0);
        if e.target_hi > 0.0 && e.load_done > e.target_hi * 1.15 {
            pts -= 2.0;
        }
        items.push((format!("Нагрузка за день — {:.0}", e.load_done), pts));
    }
    if let (Some(s), Some(n)) = (e.stress.and_then(|x| x.score), e.stress_norm) {
        let pts = (-(s - n) / 5.0).clamp(-6.0, 4.0);
        let word = if s > n + 5.0 { "выше обычного" } else if s < n - 5.0 { "ниже обычного" } else { "как обычно" };
        items.push((format!("Стресс днём {s:.0} — {word} ({n:.0})"), pts));
    }
    if hours >= 6.0 {
        let expect = ((hours - 1.0) / 14.0).clamp(0.0, 1.0) * 0.9;
        match e.kcal {
            Some(k) if e.kcal_target > 0.0 => {
                let f = k / e.kcal_target;
                if f < expect * 0.5 {
                    items.push(("Съедено заметно меньше нужного к этому часу".into(), -3.0));
                } else if f < expect * 0.75 {
                    items.push(("Еды пока маловато для этого часа".into(), -1.5));
                } else {
                    items.push(("Питание по плану".into(), 1.0));
                }
            }
            _ => {}
        }
        if let Some(w) = e.water.filter(|_| e.water_target > 0.0) {
            if w / e.water_target < expect * 0.5 {
                items.push(("Воды выпито мало".into(), -2.0));
            }
        }
    }
    if hours > 14.0 {
        items.push((format!("На ногах {:.0} ч", hours.floor()), -((hours - 14.0) * 1.0).min(4.0)));
    }
    if let Some(a) = e.alcohol.filter(|a| *a > 0.0) {
        items.push(("Алкоголь сегодня".into(), -(2.0 * a).min(6.0)));
    }
    if let Some(c) = e.caffeine.filter(|c| *c > 400.0) {
        items.push((format!("Кофеин {c:.0} мг — может скрывать усталость"), -1.0));
    }
    if let Some(t) = e.tqr {
        items.push((format!("Твоё восстановление (TQR) — {t:.0}"), (t - 13.0).clamp(-7.0, 7.0)));
    }
    let delta = items.iter().map(|x| x.1).sum::<f64>().clamp(-15.0, 15.0).round();
    let score = (m + delta).clamp(0.0, 100.0);
    let level = if score >= 67.0 { "green" } else if score >= 40.0 { "yellow" } else { "red" };
    json!({
        "score": score, "level": level, "morning": m, "delta": delta,
        "items": items.iter().map(|(t, p)| json!({"text": t, "points": (p * 10.0).round() / 10.0})).collect::<Vec<_>>(),
    })
}

/// When the evening starts (local minutes since midnight): 8 hours after today's wake-up —
/// from the watch, or your usual wake time over the last 3 weeks — but between 18:00 and 22:00.
pub fn evening_from(conn: &Connection, today: &str, tz: i64) -> i64 {
    let hyps = hypnograms(conn, &add_days(today, -21));
    let clock = |t: i64| (t + tz).rem_euclid(86400) / 60;
    let wake = match hyps.iter().find(|h| h.date == today) {
        Some(h) => Some(clock(h.end) as f64),
        None => median(&hyps.iter().map(|h| clock(h.end) as f64).collect::<Vec<_>>()),
    };
    wake.map(|w| (w as i64 + 8 * 60).clamp(18 * 60, 22 * 60)).unwrap_or(20 * 60)
}
