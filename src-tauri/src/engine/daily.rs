//! Day-level health: readiness, sleep, body trend, nutrition targets and signals.

use super::{add_days, clamp, latest, mean, median, sd, window, Profile, Series};

// ---------- readiness ----------

#[derive(Clone, Debug)]
pub struct Component {
    pub key: &'static str,
    pub label: &'static str,
    pub value: Option<f64>,
    pub baseline: Option<f64>,
    /// 0–100, 50 = your normal.
    pub score: Option<f64>,
    pub weight: f64,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct Readiness {
    pub date: String,
    pub score: Option<f64>,
    pub components: Vec<Component>,
    /// (reason, points taken off)
    pub penalties: Vec<(String, f64)>,
    pub level: &'static str,
}

pub struct HealthSeries<'a> {
    pub hrv: &'a Series,
    pub rhr: &'a Series,
    pub sleep_s: &'a Series,
    pub resp: &'a Series,
    pub breath: &'a Series,
    pub alcohol: &'a Series,
    /// Group → date the current data source started (norm is built only from then on).
    pub cuts: &'a std::collections::BTreeMap<String, String>,
}

/// The 30 days before `date`, but not earlier than the switch to the current data source.
fn hist_cut(s: &Series, date: &str, cut: Option<&String>) -> Vec<f64> {
    match cut {
        Some(c) if date >= c.as_str() => {
            let a = add_days(date, -30);
            let a = if a.as_str() < c.as_str() { c.clone() } else { a };
            s.range(a..date.to_string()).map(|(_, v)| *v).collect()
        }
        _ => window(s, date, -30, 0),
    }
}

/// Score from deviation against the personal 30-day norm.
/// `dir` = +1 when higher is better, −1 when lower is better.
fn z_score(v: f64, hist: &[f64], dir: f64, min_sd: f64) -> Option<(f64, f64)> {
    if hist.len() < 4 {
        return None;
    }
    let m = mean(hist)?;
    let s = sd(hist).unwrap_or(min_sd).max(min_sd);
    let z = (v - m) / s;
    Some((clamp(50.0 + 18.0 * z * dir, 0.0, 100.0), m))
}

/// Corridor of your norm: median ± max(0.75·sd, floor) over `hist`. Inside it a reading is
/// ordinary day-to-day noise.
pub fn corridor(hist: &[f64], floor: f64) -> Option<(f64, f64, f64)> {
    if hist.len() < 4 {
        return None;
    }
    let m = median(hist)?;
    let w = (sd(hist).unwrap_or(0.0) * 0.75).max(floor);
    Some((m, m - w, m + w))
}

/// Score 0–100 with a dead zone: inside the corridor = 50 (no effect), beyond it 18 points per
/// standard deviation (at least `floor`) past the edge, in the good or bad direction.
fn corr_score(v: f64, hist: &[f64], dir: f64, floor: f64) -> Option<(f64, f64)> {
    let (m, lo, hi) = corridor(hist, floor)?;
    let s = sd(hist).unwrap_or(floor).max(floor);
    let e = if v > hi { v - hi } else if v < lo { v - lo } else { 0.0 };
    Some((clamp(50.0 + 18.0 * dir * e / s, 0.0, 100.0), m))
}

fn fmt1(x: f64) -> String {
    format!("{x:.1}").replace('.', ",")
}

pub fn readiness(date: &str, h: &HealthSeries, tsb: Option<f64>) -> Readiness {
    let mut comps = Vec::new();

    // HRV: higher than usual = recovered.
    let hrv = h.hrv.get(date).copied();
    let hist = hist_cut(h.hrv, date, h.cuts.get("hrv"));
    let r = hrv.and_then(|v| corr_score(v, &hist, 1.0, 6.0));
    comps.push(Component {
        key: "hrv",
        label: "HRV",
        value: hrv,
        baseline: r.map(|x| x.1).or_else(|| mean(&hist)),
        score: r.map(|x| x.0),
        weight: 0.30,
        note: match (hrv, r) {
            (Some(v), Some((_, m))) => format!("{:.0} мс при норме {:.0}", v, m),
            (Some(v), None) => format!("{v:.0} мс — норма ещё набирается"),
            _ => "нет данных за ночь".into(),
        },
    });

    // Resting HR: lower than usual = recovered.
    let rhr = h.rhr.get(date).copied();
    let hist = hist_cut(h.rhr, date, h.cuts.get("rhr"));
    let r = rhr.and_then(|v| corr_score(v, &hist, -1.0, 2.0));
    comps.push(Component {
        key: "rhr",
        label: "Пульс покоя",
        value: rhr,
        baseline: r.map(|x| x.1).or_else(|| mean(&hist)),
        score: r.map(|x| x.0),
        weight: 0.20,
        note: match (rhr, r) {
            (Some(v), Some((_, m))) => format!("{:.0} уд/мин при норме {:.0}", v, m),
            (Some(v), None) => format!("{v:.0} уд/мин — норма ещё набирается"),
            _ => "нет данных".into(),
        },
    });

    // Sleep: absolute duration (8 h = full marks) blended with your own norm.
    let sl = h.sleep_s.get(date).copied();
    let hist = hist_cut(h.sleep_s, date, h.cuts.get("sleep"));
    let sleep_score = sl.map(|s| {
        let abs = clamp((s / 3600.0 - 5.0) / 3.0 * 100.0, 0.0, 100.0);
        match z_score(s, &hist, 1.0, 1800.0) {
            Some((z, _)) => 0.6 * abs + 0.4 * z,
            None => abs,
        }
    });
    comps.push(Component {
        key: "sleep",
        label: "Сон",
        value: sl,
        baseline: mean(&hist),
        score: sleep_score,
        weight: 0.25,
        note: match sl {
            Some(s) => format!("{} ч {} мин", (s / 3600.0) as i64, ((s % 3600.0) / 60.0).round() as i64),
            None => "сон ещё не записан".into(),
        },
    });

    // Breathing during sleep: higher rate or more disturbances = worse.
    let resp = h.resp.get(date).copied();
    let hist = hist_cut(h.resp, date, h.cuts.get("vitals"));
    let r = resp.and_then(|v| corr_score(v, &hist, -1.0, 0.6));
    comps.push(Component {
        key: "resp",
        label: "Дыхание",
        value: resp,
        baseline: r.map(|x| x.1),
        score: r.map(|x| x.0),
        weight: 0.05,
        note: resp.map(|v| format!("{} вдохов/мин", fmt1(v))).unwrap_or_else(|| "нет данных".into()),
    });
    // Breathing disturbances jump from night to night; they are a signal over several nights
    // (see `signals`), not part of the daily score.

    // Training form: very negative TSB = accumulated fatigue.
    let ts = tsb.map(|t| clamp(62.0 + 1.6 * t, 0.0, 100.0));
    comps.push(Component {
        key: "form",
        label: "Свежесть после нагрузки",
        value: tsb,
        baseline: None,
        score: ts,
        weight: 0.15,
        note: tsb.map(|t| format!("форма {:+.0}", t)).unwrap_or_else(|| "нет тренировок".into()),
    });

    let have_core = comps.iter().any(|c| (c.key == "hrv" || c.key == "rhr") && c.score.is_some());
    let wsum: f64 = comps.iter().filter(|c| c.score.is_some()).map(|c| c.weight).sum();
    let mut score = if have_core && wsum > 0.0 {
        Some(comps.iter().filter_map(|c| c.score.map(|s| s * c.weight)).sum::<f64>() / wsum)
    } else {
        None
    };

    // Alcohol the evening before.
    let mut penalties = Vec::new();
    let yday = add_days(date, -1);
    let drinks = h.alcohol.get(&yday).copied().unwrap_or(0.0);
    if drinks > 0.0 {
        // HRV and resting HR already show most of the effect, so only a small extra deduction.
        let p = clamp(2.0 + drinks, 3.0, 10.0);
        penalties.push((format!("алкоголь накануне ({})", fmt1(drinks)), p));
        if let Some(s) = score.as_mut() {
            *s -= p;
        }
    }
    let score = score.map(|s| clamp(s, 1.0, 100.0).round());
    let level = match score {
        None => "unknown",
        Some(s) if s >= 67.0 => "green",
        Some(s) if s >= 40.0 => "yellow",
        Some(_) => "red",
    };
    Readiness { date: date.to_string(), score, components: comps, penalties, level }
}

// ---------- sleep ----------

#[derive(Clone, Debug)]
pub struct Night {
    pub date: String,
    pub total_s: f64,
    pub deep_s: f64,
    pub rem_s: f64,
    pub core_s: f64,
    pub awake_s: f64,
    pub score: f64,
}

pub fn night(date: &str, core: &Series, deep: &Series, rem: &Series, awake: &Series) -> Option<Night> {
    let c = core.get(date).copied().unwrap_or(0.0);
    let d = deep.get(date).copied().unwrap_or(0.0);
    let r = rem.get(date).copied().unwrap_or(0.0);
    let total = c + d + r;
    if total < 1800.0 {
        return None;
    }
    let a = awake.get(date).copied().unwrap_or(0.0);
    let dur = clamp((total / 3600.0 - 4.5) / 3.5, 0.0, 1.0) * 60.0;
    let deep_pts = clamp(d / total / 0.15, 0.0, 1.0) * 15.0;
    let rem_pts = clamp(r / total / 0.20, 0.0, 1.0) * 15.0;
    let eff = total / (total + a);
    let eff_pts = clamp((eff - 0.80) / 0.15, 0.0, 1.0) * 10.0;
    Some(Night {
        date: date.to_string(),
        total_s: total,
        deep_s: d,
        rem_s: r,
        core_s: c,
        awake_s: a,
        score: (dur + deep_pts + rem_pts + eff_pts).round(),
    })
}

// ---------- body ----------

#[derive(Clone, Debug, Default)]
pub struct Trend {
    pub latest: Option<(String, f64)>,
    pub smooth: Option<f64>,
    pub change_7d: Option<f64>,
    pub change_30d: Option<f64>,
}

/// Exponentially smoothed trend that ignores day-to-day noise of the scales.
pub fn trend(s: &Series, today: &str) -> Trend {
    let pts: Vec<(&String, &f64)> = s.range(..add_days(today, 1)).collect();
    if pts.is_empty() {
        return Trend::default();
    }
    let mut ew: Option<f64> = None;
    let mut by_day = Series::new();
    for (d, v) in &pts {
        ew = Some(match ew {
            None => **v,
            Some(e) => e + (**v - e) * 0.15,
        });
        by_day.insert((*d).clone(), ew.unwrap_or(**v));
    }
    let now = ew;
    let at = |k: i64| latest(&by_day, &add_days(today, -k), 21).map(|x| x.1);
    Trend {
        latest: pts.last().map(|(d, v)| ((*d).clone(), **v)),
        smooth: now,
        change_7d: now.zip(at(7)).map(|(a, b)| a - b),
        change_30d: now.zip(at(30)).map(|(a, b)| a - b),
    }
}

// ---------- nutrition ----------

#[derive(Clone, Debug, Default)]
pub struct Targets {
    pub energy_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: f64,
    pub water_ml: f64,
    pub bmr_kcal: f64,
    /// Everyday movement on top of the basal rate.
    pub neat_kcal: f64,
    /// Share of the workouts' calories counted in.
    pub train_kcal: f64,
    /// Deficit (−) or surplus (+) for the weight goal.
    pub goal_kcal: f64,
}

/// Maintenance targets for a day; `train_kcal`/`train_min` are that day's workouts.
pub fn targets(p: &Profile, weight: f64, year: i64, train_kcal: f64, train_min: f64) -> Targets {
    let age = (year - p.birth_year) as f64;
    let bmr = 10.0 * weight + 6.25 * p.height_cm - 5.0 * age + if p.male { 5.0 } else { -161.0 };
    // Everyday life (NEAT) ×1.3, plus most of what the workouts burned.
    let neat = bmr * 0.3;
    let train = train_kcal * 0.8;
    let base = bmr + neat + train;
    // 1 kg of body fat ≈ 7700 kcal; the deficit is capped at 25% so training does not suffer.
    let goal = match p.goal.as_str() {
        "lose" => -(p.goal_rate * 7700.0 / 7.0).min(base * 0.25),
        "gain" => (p.goal_rate * 7700.0 / 7.0).min(500.0),
        _ => 0.0,
    };
    let energy = base + goal;
    // Protein and fat per kg; carbohydrates fill the rest, so they grow on hard days.
    let protein = p.protein_g_per_kg * weight;
    let mut fat = (0.9 * weight).min(energy * 0.30 / 9.0);
    let mut carbs = (energy - protein * 4.0 - fat * 9.0) / 4.0;
    let carb_floor = if train_min >= 45.0 { 3.0 } else { 2.0 } * weight;
    if carbs < carb_floor {
        // Not enough room: trim fat down to 0.6 g/kg before carbohydrates.
        fat = ((energy - protein * 4.0 - carb_floor * 4.0) / 9.0).max(0.6 * weight);
        carbs = ((energy - protein * 4.0 - fat * 9.0) / 4.0).max(0.0);
    }
    Targets {
        energy_kcal: energy.round(),
        protein_g: protein.round(),
        carbs_g: carbs.round(),
        fat_g: fat.round(),
        fiber_g: (energy / 1000.0 * 14.0).min(45.0).round(),
        water_ml: (35.0 * weight + train_min * 10.0).round(),
        bmr_kcal: bmr.round(),
        neat_kcal: neat.round(),
        train_kcal: train.round(),
        goal_kcal: goal.round(),
    }
}

// ---------- signals ----------

#[derive(Clone, Debug)]
pub struct Signal {
    /// "warn" | "info" | "good"
    pub level: &'static str,
    pub code: &'static str,
    pub text: String,
}

pub struct SignalInput<'a> {
    /// (how many recent runs went above the profile max HR, highest pulse seen)
    pub over_max: Option<(usize, f64)>,
    pub hr_max: f64,
    pub today: &'a str,
    pub rhr: &'a Series,
    pub hrv: &'a Series,
    pub sleep_s: &'a Series,
    pub food_kcal: &'a Series,
    pub acwr: Option<f64>,
    pub zones_age_days: Option<i64>,
    pub weight_change_30d: Option<f64>,
    pub breath: &'a Series,
}

pub fn signals(i: &SignalInput) -> Vec<Signal> {
    let mut out = Vec::new();
    let t = i.today;
    if let (Some(v), Some(m)) = (i.rhr.get(t), median(&window(i.rhr, t, -30, 0))) {
        if window(i.rhr, t, -30, 0).len() >= 4 && *v - m >= 5.0 {
            out.push(Signal { level: "warn", code: "rhr_up", text: format!("Пульс покоя {:.0} — на {:.0} выше твоей нормы", v, v - m) });
        }
    }
    if let (Some(v), Some(m)) = (i.hrv.get(t), median(&window(i.hrv, t, -30, 0))) {
        if window(i.hrv, t, -30, 0).len() >= 4 && *v < m * 0.75 {
            out.push(Signal { level: "warn", code: "hrv_down", text: format!("HRV {:.0} мс — на {:.0}% ниже нормы", v, (1.0 - v / m) * 100.0) });
        }
    }
    // Low pulse together with low HRV: can be deep accumulated fatigue, not great recovery.
    if let (Some(r), Some(h), Some((_, rlo, _)), Some((_, hlo, _))) =
        (i.rhr.get(t), i.hrv.get(t), corridor(&window(i.rhr, t, -30, 0), 2.0), corridor(&window(i.hrv, t, -30, 0), 6.0))
    {
        if *r < rlo && *h < hlo {
            out.push(Signal { level: "warn", code: "low_rhr_low_hrv", text: "Пульс покоя ниже нормы и HRV тоже ниже — так бывает при накопленной усталости, лучше лёгкий день".into() });
        }
    }
    // Breathing disturbances above your corridor on 3 of the last 4 nights.
    if let Some((_, _, hi)) = corridor(&window(i.breath, t, -30, -3), 1.0) {
        let last: Vec<f64> = window(i.breath, t, -3, 1);
        if last.len() >= 3 && last.iter().filter(|v| **v > hi).count() >= 3 {
            out.push(Signal { level: "info", code: "breath_up", text: "Нарушения дыхания во сне выше твоей нормы несколько ночей подряд — проверь, не заложен ли нос, не душно ли в спальне; если держится неделями — стоит сказать врачу".into() });
        }
    }
    let short: Vec<f64> = window(i.sleep_s, t, -2, 1).into_iter().filter(|s| *s < 6.0 * 3600.0).collect();
    if short.len() >= 2 {
        out.push(Signal { level: "warn", code: "sleep_short", text: "Две из последних трёх ночей — меньше 6 часов сна".into() });
    }
    match i.acwr {
        Some(r) if r > 1.5 => out.push(Signal { level: "warn", code: "load_spike", text: format!("Нагрузка за неделю в {:.1} раза выше обычной — риск перегруза", r).replace('.', ",") }),
        Some(r) if r > 1.3 => out.push(Signal { level: "info", code: "load_high", text: format!("Нагрузка за неделю выше обычной ({:.2})", r).replace('.', ",") }),
        _ => {}
    }
    let yday = add_days(t, -1);
    if i.food_kcal.get(&yday).map(|k| *k < 1200.0).unwrap_or(true) {
        out.push(Signal { level: "info", code: "food_missing", text: "Вчера почти нет записей еды в YAZIO — питание посчитано неполно".into() });
    }
    if let Some(d) = i.zones_age_days {
        if d > 120 {
            out.push(Signal { level: "info", code: "zones_old", text: format!("Зонам {d} дней — пора повторить тест или уточнить пороги") });
        }
    }
    if let Some((n, top)) = i.over_max {
        if n >= 3 {
            out.push(Signal {
                level: "info",
                code: "hr_over_max",
                text: format!(
                    "В {n} пробежках пульс был выше максимума из профиля ({:.0}), до {:.0}. Если это не сбой датчика — можно поднять максимум в Профиле или пересдать тест",
                    i.hr_max, top
                ),
            });
        }
    }
    if let Some(w) = i.weight_change_30d {
        if w.abs() >= 1.0 {
            out.push(Signal {
                level: "info",
                code: "weight_trend",
                text: format!("Вес за 30 дней {} {} кг", if w < 0.0 { "−" } else { "+" }, fmt1(w.abs())),
            });
        }
    }
    out
}

// ---------- day recommendation (rules, until the Claude coach arrives) ----------

pub struct Advice {
    pub title: String,
    pub text: String,
    /// Multiplier of the usual daily load that suits today.
    pub load_factor: f64,
}

#[allow(dead_code)]
pub fn advice(level: &str, score: Option<f64>, acwr: Option<f64>, zones: &[f64; 6], drank: bool) -> Advice {
    let z2 = zones[2] as i64;
    let z1 = zones[1] as i64;
    let high = acwr.map(|a| a > 1.3).unwrap_or(false);
    match level {
        "red" => Advice {
            title: "День восстановления".into(),
            text: format!(
                "Готовность {} — организм ещё восстанавливается{}. Прогулка 30–40 минут или мобилити, пульс до {z1}. Без интенсива, больше воды, лечь пораньше.",
                score.map(|s| s as i64).unwrap_or(0),
                if drank { " после алкоголя" } else { "" }
            ),
            load_factor: 0.4,
        },
        "yellow" => Advice {
            title: "Лёгкая аэробная тренировка".into(),
            text: format!("Бег или ходьба 40–60 минут во второй зоне, пульс до {z2}. Качественную работу лучше перенести на день с зелёной готовностью."),
            load_factor: 0.8,
        },
        "green" if high => Advice {
            title: "Готов, но неделя уже тяжёлая".into(),
            text: format!("Восстановление хорошее, но нагрузка за неделю выше обычной. Спокойный объём в Z2 (до {z2}), без интервалов."),
            load_factor: 0.9,
        },
        "green" => Advice {
            title: "Можно качественную тренировку".into(),
            text: "Темповая или интервалы — по плану. Разминка 15 минут, заминка 10. Если на разминке пульс выше обычного — оставь лёгкий бег.".into(),
            load_factor: 1.3,
        },
        _ => Advice {
            title: "Мало данных для совета".into(),
            text: "Нужны HRV и пульс покоя за несколько ночей — носи часы во сне, и через пару дней Kouros начнёт советовать.".into(),
            load_factor: 1.0,
        },
    }
}

// ---------- plan for the day (0.10.2) ----------

/// Steps from rest to intervals. The body score picks the step; safety gates and how you feel
/// (questionnaires) can only move it down.
pub const TIERS: [&str; 7] = ["rest", "light", "recovery", "easy", "aerobic", "tempo", "intervals"];

pub fn tier_of(score: f64) -> usize {
    if score >= 70.0 {
        6
    } else if score >= 65.0 {
        5
    } else if score >= 55.0 {
        4
    } else if score >= 45.0 {
        3
    } else if score >= 35.0 {
        2
    } else if score >= 20.0 {
        1
    } else {
        0
    }
}

/// Share of the usual day for a tier; from 45 up it changes smoothly with the score.
pub fn tier_factors(tier: usize, score: Option<f64>) -> (f64, f64) {
    let lerp = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let s = score.unwrap_or(50.0);
    match tier {
        0 => (0.0, 0.15),
        1 => (0.15, 0.3),
        2 => (0.3, 0.5),
        3 => lerp((0.75, 1.0), (0.85, 1.15), ((s - 45.0) / 10.0).clamp(0.0, 1.0)),
        4 => lerp((0.85, 1.15), (0.95, 1.2), ((s - 55.0) / 10.0).clamp(0.0, 1.0)),
        5 => lerp((0.95, 1.2), (1.0, 1.3), ((s - 65.0) / 5.0).clamp(0.0, 1.0)),
        _ => (1.0, 1.3),
    }
}

pub struct Gates {
    pub week_heavy: bool,
    pub quality_yesterday: bool,
    pub body_warning: bool,
    /// Questionnaires today: 0 = nothing / fine, 1 = no quality, 2 = one step down, 3 = two steps down.
    pub feel: u8,
    pub feel_reason: Option<String>,
    pub has_survey: bool,
    pub drank: bool,
}

pub struct DayPlan {
    pub tier: usize,
    pub body_tier: usize,
    pub title: String,
    pub text: String,
    pub reasons: Vec<String>,
    pub hint: Option<String>,
    pub lo: f64,
    pub hi: f64,
}

pub fn plan_day(score: Option<f64>, zones: &[f64; 6], g: &Gates) -> DayPlan {
    let z1 = zones[1] as i64;
    let z2 = zones[2] as i64;
    let Some(sc) = score else {
        return DayPlan {
            tier: 3, body_tier: 3,
            title: "Мало данных для совета".into(),
            text: "Нужны HRV и пульс покоя за несколько ночей — носи часы во сне, и через пару дней Kouros начнёт советовать.".into(),
            reasons: vec![], hint: None, lo: 0.6, hi: 1.0,
        };
    };
    let body = tier_of(sc);
    let mut t = body;
    let mut reasons = Vec::new();
    // Feelings first: they can take whole steps down.
    if g.feel >= 2 && t > 0 {
        let down = if g.feel >= 3 { 2 } else { 1 };
        t = t.saturating_sub(down);
        if let Some(r) = &g.feel_reason {
            reasons.push(r.clone());
        }
    }
    // Quality (tempo/intervals) needs every gate open.
    if t >= 5 {
        let mut why = Vec::new();
        if g.week_heavy {
            why.push("нагрузка за неделю уже выше обычной".to_string());
        }
        if g.quality_yesterday {
            why.push("вчера была интенсивная тренировка".to_string());
        }
        if g.body_warning {
            why.push("пульс или HRV подают тревожный сигнал".to_string());
        }
        if g.feel == 1 {
            if let Some(r) = &g.feel_reason {
                why.push(r.clone());
            }
        }
        if !why.is_empty() {
            t = 4;
            reasons.extend(why);
        }
    }
    let (mut lo, mut hi) = tier_factors(t, Some(sc));
    if t < body {
        // Stepped down: use the lower tier's range as is, not the body score's position in it.
        let f = tier_factors(t, Some(if t >= 3 { [0.0, 0.0, 0.0, 50.0, 60.0, 67.0, 75.0][t] } else { 0.0 }));
        lo = f.0;
        hi = f.1;
    }
    let (title, text) = match t {
        6 => ("Можно интервалы".to_string(), format!("Тело хорошо восстановилось. Интервалы или темп по плану: разминка 15 минут, заминка 10. Если на разминке пульс выше обычного — оставь лёгкий бег до {z2}.")),
        5 => ("Можно темповую".to_string(), format!("Готовность выше нормы — подходит темповая работа. Интервалы лучше оставить на день, когда готовность от 70. Разминка и заминка в Z2 (до {z2}).")),
        4 => ("Обычный аэробный день".to_string(), format!("Обычный объём во второй зоне, пульс до {z2}. Силовая — можно.")),
        3 => ("Обычный объём, без интенсива".to_string(), format!("День в пределах твоей нормы. Бег или ходьба в Z2 до {z2}, без интервалов и темпа.")),
        2 => ("Только восстановление".to_string(), format!("Показатели заметно ниже нормы. Длинная спокойная прогулка (пульс до {z1}), растяжка, мобилити. Без бега и силовой.")),
        1 => ("Очень лёгкий день".to_string(), "Короткая прогулка 20–40 минут, лёгкая растяжка и дыхание. Лечь пораньше.".to_string()),
        _ => ("День отдыха".to_string(), "Организму тяжело — тренировки лучше отменить. Отдых, вода, сон. Если пульс покоя или температура тоже выше нормы — возможно, начинается болезнь.".to_string()),
    };
    let text = if g.drank && t >= 2 { format!("{text} После алкоголя — больше воды.") } else { text };
    let hint = (t >= 5 && !g.has_survey).then(|| "Перед качественной тренировкой отметь TQR во вкладке «Психология» — это 5 секунд.".to_string());
    DayPlan { tier: t, body_tier: body, title, text, reasons, hint, lo, hi }
}

// ---------- target load for the day ----------

#[derive(Clone, Debug)]
pub struct LoadTarget {
    pub lo: f64,
    pub hi: f64,
    /// What the week can still take before crossing the cap (None = too little history).
    pub headroom: Option<f64>,
    /// True when the weekly cap, not readiness, set the upper bound.
    pub capped: bool,
}

/// A range around the usual daily load set by readiness, with the top limited by the week.
/// `chronic_week` = average week of the last 28 days; `used6` = load of the previous 6 days.
pub fn load_target(flo: f64, fhi: f64, usual_day: f64, week_cap: f64, chronic_week: Option<f64>, used6: f64) -> LoadTarget {
    let mut lo = usual_day * flo;
    let mut hi = usual_day * fhi;
    let headroom = chronic_week.map(|w| (w * week_cap - used6).max(0.0));
    let mut capped = false;
    if let Some(h) = headroom {
        if h < hi {
            hi = h;
            capped = true;
            if lo > hi {
                lo = hi * 0.6;
            }
        }
    }
    LoadTarget { lo: lo.round(), hi: hi.round(), headroom: headroom.map(|h| h.round()), capped }
}
