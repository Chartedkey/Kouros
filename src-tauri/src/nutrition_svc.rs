//! Nutrition tab: targets for the day and where they come from, meals by time
//! (YAZIO entries via Apple Health), diet quality, eating window, micronutrients,
//! record completeness and the history of eaten vs spent.

use crate::engine::{add_days, day_num, mean, median, Series};
use crate::engine_svc::{self, series};
use crate::health_svc::iso;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Eating day starts at 05:00 local, so a 00:30 snack belongs to the evening before.
const DAY_SHIFT: i64 = 5 * 3600;

fn eat_day(t: i64, tz: i64) -> String {
    crate::engine::day_of(t - DAY_SHIFT, tz)
}

fn clock(t: i64, tz: i64) -> String {
    let m = (t + tz).rem_euclid(86400) / 60;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Minutes after 05:00 of the eating day (so 00:30 → 1170).
fn eat_min(t: i64, tz: i64) -> i64 {
    (t + tz - DAY_SHIFT).rem_euclid(86400) / 60
}

/// Training calories (counted 80% in targets) and minutes per local day.
fn train_by_day(conn: &Connection, tz: i64, from: &str) -> BTreeMap<String, (f64, f64)> {
    let since = day_num(from).map(|d| d * 86400 - tz).unwrap_or(0);
    let mut out: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let Ok(mut st) = conn.prepare(
        "SELECT a.start_utc, a.calories, COALESCE(a.moving_s, a.elapsed_s), m.load FROM va a
         LEFT JOIN activity_metrics m ON m.activity_id = a.id WHERE a.start_utc >= ?1",
    ) else {
        return out;
    };
    let rows = st.query_map(params![since], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, Option<f64>>(2)?, r.get::<_, Option<f64>>(3)?))
    });
    if let Ok(rows) = rows {
        for (t, kcal, dur, load) in rows.flatten() {
            let e = out.entry(crate::engine::day_of(t, tz)).or_insert((0.0, 0.0));
            e.0 += kcal.unwrap_or(load.unwrap_or(0.0) * 4.0);
            e.1 += dur.unwrap_or(0.0) / 60.0;
        }
    }
    out
}

struct Item {
    ts: i64,
    name: String,
}

/// Food entries (name + nutrients) grouped by their timestamp.
fn entries(conn: &Connection, from: &str) -> (Vec<Item>, BTreeMap<i64, BTreeMap<String, f64>>) {
    let mut items = Vec::new();
    let mut nut: BTreeMap<i64, BTreeMap<String, f64>> = BTreeMap::new();
    if let Ok(mut st) = conn.prepare("SELECT ts, txt FROM hm WHERE metric = 'food_entry_name' AND date >= ?1 AND txt IS NOT NULL") {
        if let Ok(rows) = st.query_map(params![from], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            for (ts, name) in rows.flatten() {
                if let Some(t) = iso(&ts) {
                    items.push(Item { ts: t, name });
                }
            }
        }
    }
    if let Ok(mut st) = conn.prepare(
        "SELECT ts, metric, num FROM hm WHERE metric IN
         ('food_entry_energy', 'food_entry_protein', 'food_entry_carbohydrates', 'food_entry_fat_total', 'food_entry_fiber', 'food_entry_sugar')
         AND date >= ?1 AND num IS NOT NULL",
    ) {
        if let Ok(rows) = st.query_map(params![from], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))) {
            for (ts, m, v) in rows.flatten() {
                if let Some(t) = iso(&ts) {
                    *nut.entry(t).or_default().entry(m.trim_start_matches("food_entry_").to_string()).or_insert(0.0) += v;
                }
            }
        }
    }
    items.sort_by_key(|i| i.ts);
    (items, nut)
}

/// Meals of one eating day: entries within 25 minutes form one meal.
fn meals(items: &[Item], nut: &BTreeMap<i64, BTreeMap<String, f64>>, day: &str, tz: i64) -> Vec<Value> {
    let mut groups: Vec<Vec<&Item>> = Vec::new();
    for it in items.iter().filter(|i| eat_day(i.ts, tz) == day) {
        match groups.last_mut() {
            Some(g) if it.ts - g.last().map(|x| x.ts).unwrap_or(0) <= 25 * 60 => g.push(it),
            _ => groups.push(vec![it]),
        }
    }
    let mut dinners = 0;
    groups
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let t0 = g[0].ts;
            let mut stamps: Vec<i64> = g.iter().map(|i| i.ts).collect();
            stamps.dedup();
            let sum = |key: &str| stamps.iter().filter_map(|t| nut.get(t).and_then(|m| m.get(key))).sum::<f64>();
            let kcal = sum("energy");
            let m = eat_min(t0, tz) + 300; // minutes since local midnight, may exceed 1440 after midnight
            let label = if k == 0 && m >= 11 * 60 {
                "Первый приём"
            } else if m >= 24 * 60 {
                "Ночной перекус"
            } else if kcal < 300.0 {
                "Перекус"
            } else if m < 11 * 60 {
                "Завтрак"
            } else if m < 16 * 60 {
                "Обед"
            } else {
                dinners += 1;
                if dinners > 1 { "Второй ужин" } else { "Ужин" }
            };
            let mut names: Vec<String> = g.iter().map(|i| i.name.clone()).collect();
            names.dedup();
            json!({"time": clock(t0, tz), "ts": t0, "label": label, "names": names, "kcal": kcal.round(),
                   "protein": r1(sum("protein")), "carbs": r1(sum("carbohydrates")), "fat": r1(sum("fat_total"))})
        })
        .collect()
}

/// Average daily intake from food over the last 7 days by metric (with estimates, without
/// supplements) — for the safe-upper-limit check in «Витамины и лекарства».
pub fn food7(conn: &Connection) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if let Ok(v) = summary(conn) {
        for m in v["micro"].as_array().cloned().unwrap_or_default() {
            if let (Some(k), Some(a)) = (m["key"].as_str(), m["avgEst"].as_f64().or(m["avg"].as_f64())) {
                out.insert(k.to_string(), a);
            }
        }
    }
    out
}

/// Which country's norms to use (settings `nutrient_norms`, default Russia).
pub fn norms_country(conn: &Connection) -> String {
    crate::db::get_setting(conn, "nutrient_norms")
        .and_then(|s| serde_json::from_str::<Value>(&s).ok().and_then(|v| v.as_str().map(String::from)).or(Some(s)))
        .filter(|c| crate::norms::COUNTRIES.iter().any(|x| x.0 == c))
        .unwrap_or_else(|| "ru".into())
}

/// Norms in food_est::NUTS order (fiber has none — 0 means "no cap").
fn caps_for(country: &str) -> [f64; 22] {
    let mut c = [0.0; 22];
    for (i, (_, m)) in crate::food_est::NUTS.iter().enumerate() {
        c[i] = crate::norms::norm(m, country).unwrap_or(0.0);
    }
    c
}

/// Recommended daily intake for an adult man (micrograms or milligrams as in YAZIO).
const MICRO: [(&str, &str, &str, f64); 21] = [
    ("dietary_vitamin_a_total", "Витамин A", "мкг", 900.0),
    ("dietary_vitamin_c_total", "Витамин C", "мг", 90.0),
    ("dietary_vitamin_d_total", "Витамин D", "мкг", 15.0),
    ("dietary_vitamin_e_total", "Витамин E", "мг", 15.0),
    ("dietary_vitamin_k_total", "Витамин K", "мкг", 120.0),
    ("dietary_thiamin_total", "B1 (тиамин)", "мг", 1.2),
    ("dietary_riboflavin_total", "B2 (рибофлавин)", "мг", 1.3),
    ("dietary_niacin_total", "B3 (ниацин)", "мг", 16.0),
    ("dietary_vitamin_b6_total", "B6", "мг", 1.3),
    ("dietary_folate_total", "B9 (фолат)", "мкг", 400.0),
    ("dietary_vitamin_b12_total", "B12", "мкг", 2.4),
    ("dietary_calcium_total", "Кальций", "мг", 1000.0),
    ("dietary_iron_total", "Железо", "мг", 8.0),
    ("dietary_magnesium_total", "Магний", "мг", 420.0),
    ("dietary_zinc_total", "Цинк", "мг", 11.0),
    ("dietary_potassium_total", "Калий", "мг", 3400.0),
    ("dietary_selenium_total", "Селен", "мкг", 55.0),
    ("dietary_phosphorus_total", "Фосфор", "мг", 700.0),
    ("dietary_copper_total", "Медь", "мг", 0.9),
    ("dietary_manganese_total", "Марганец", "мг", 2.3),
    ("dietary_sodium_total", "Натрий", "мг", 1500.0),
];

const VEG: [&str; 16] = [
    "салат", "овощ", "огур", "томат", "помидор", "брокк", "капуст", "перец", "кабач", "баклаж", "морков", "свекл", "шпинат", "фасол", "нут", "чечев",
];

pub fn summary(conn: &Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let tz = p.tz_offset_s;
    let today = engine_svc::today(&p);
    let year: i64 = today.get(0..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    let from90 = add_days(&today, -90);

    let kcal = series(conn, "dietary_energy_total", "max");
    let prot = series(conn, "dietary_protein_total", "max");
    let carb = series(conn, "dietary_carbohydrates_total", "max");
    let fatg = series(conn, "dietary_fat_total_total", "max");
    let sat = series(conn, "dietary_fat_saturated_total", "max");
    let fib = series(conn, "dietary_fiber_total", "max");
    let sug = series(conn, "dietary_sugar_total", "max");
    let water = series(conn, "dietary_water_total", "max");
    let active = series(conn, "active_energy_total", "max");
    let basal = series(conn, "basal_energy_total", "max");
    let weight = series(conn, "body_mass", "median");
    let w_now = crate::engine::daily::trend(&weight, &today).smooth.or_else(|| weight.values().next_back().copied()).unwrap_or(100.0);
    let train = train_by_day(conn, tz, &from90);

    let tgt = |d: &str| {
        let (tk, tm) = train.get(d).copied().unwrap_or((0.0, 0.0));
        (crate::engine::daily::targets(&p, w_now, year, tk, tm), tm)
    };
    let (t, tmin) = tgt(&today);
    // Estimated fiber and micros for entries that came with macros only.
    let country = norms_country(conn);
    let caps = caps_for(&country);
    let est = crate::food_est::estimate(conn, &add_days(&from90, -1), tz, &caps);
    let est_of = |d: &str, i: usize| est.get(d).map(|e| e.add[i]).unwrap_or(0.0);
    let supp_of = |d: &str, i: usize| est.get(d).map(|e| e.supp[i]).unwrap_or(0.0);
    let eaten_today = json!({"kcal": kcal.get(&today), "protein": prot.get(&today), "carbs": carb.get(&today), "fat": fatg.get(&today),
                             "fiber": fib.get(&today), "fiberEst": r1(est_of(&today, 0)), "waterMl": water.get(&today)});

    // ----- days (90) -----
    let mut days: Vec<Value> = Vec::new();
    let mut complete: BTreeMap<String, i64> = BTreeMap::new();
    let (items, nut) = entries(conn, &add_days(&from90, -1));
    for k in (0..90).rev() {
        let d = add_days(&today, -k);
        let (td, _) = tgt(&d);
        let e = kcal.get(&d).copied();
        // 2 = full day, 1 = probably not everything was logged, 0 = no records.
        let status = match e {
            None => 0,
            Some(v) if v >= 0.6 * td.energy_kcal => 2,
            Some(v) if v > 0.0 => 1,
            _ => 0,
        };
        complete.insert(d.clone(), status);
        let spent = match (active.get(&d), basal.get(&d)) {
            (Some(a), Some(b)) => Some((a + b).round()),
            (Some(a), None) => Some((a + td.bmr_kcal).round()),
            _ => None,
        };
        days.push(json!({
            "date": d, "kcal": e.map(|v| v.round()), "protein": prot.get(&d).map(|v| v.round()), "carbs": carb.get(&d).map(|v| v.round()),
            "fat": fatg.get(&d).map(|v| v.round()), "fiber": fib.get(&d).map(|v| r1(*v)), "sugar": sug.get(&d).map(|v| v.round()),
            "satFat": sat.get(&d).map(|v| r1(*v)), "waterMl": water.get(&d).map(|v| v.round()), "spent": spent, "status": status,
            "fiberEst": r1(est_of(&d, 0)),
            "target": {"kcal": td.energy_kcal, "protein": td.protein_g, "carbs": td.carbs_g, "fat": td.fat_g, "fiber": td.fiber_g, "waterMl": td.water_ml},
        }));
    }

    // ----- quality over the full days of the last 30 -----
    let full30: Vec<String> = complete.iter().filter(|(d, s)| **s == 2 && **d >= add_days(&today, -30)).map(|(d, _)| d.clone()).collect();
    let avg_of = |s: &Series| mean(&full30.iter().filter_map(|d| s.get(d).copied()).collect::<Vec<_>>());
    let k_avg = avg_of(&kcal);
    let fiber_with_est = mean(&full30.iter().map(|d| fib.get(d).copied().unwrap_or(0.0) + est_of(d, 0)).collect::<Vec<_>>());
    let fat_pct = match (avg_of(&fatg), k_avg) {
        (Some(f), Some(k)) if k > 0.0 => Some((f * 9.0 / k * 100.0).round()),
        _ => None,
    };
    let sug_pct = match (avg_of(&sug), k_avg) {
        (Some(s), Some(k)) if k > 0.0 => Some((s * 4.0 / k * 100.0).round()),
        _ => None,
    };
    let veg_days = full30
        .iter()
        .filter(|d| items.iter().any(|i| eat_day(i.ts, tz) == **d && VEG.iter().any(|v| i.name.to_lowercase().contains(v))))
        .count();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for it in items.iter().filter(|i| eat_day(i.ts, tz) >= add_days(&today, -30)) {
        *counts.entry(it.name.trim().to_string()).or_default() += 1;
    }
    let mut top: Vec<(String, usize)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<Value> = top.into_iter().take(10).map(|(n, c)| json!([n, c])).collect();

    // ----- eating window, last 30 eating days -----
    let mut win: BTreeMap<String, (i64, i64, String, String)> = BTreeMap::new();
    for it in &items {
        let d = eat_day(it.ts, tz);
        if d < add_days(&today, -30) {
            continue;
        }
        let m = eat_min(it.ts, tz);
        let e = win.entry(d).or_insert((m, m, clock(it.ts, tz), clock(it.ts, tz)));
        if m < e.0 {
            e.0 = m;
            e.2 = clock(it.ts, tz);
        }
        if m > e.1 {
            e.1 = m;
            e.3 = clock(it.ts, tz);
        }
    }
    let firsts: Vec<f64> = win.values().map(|w| w.0 as f64).collect();
    let lasts: Vec<f64> = win.values().map(|w| w.1 as f64).collect();
    let hhmm = |m: f64| {
        let m = (m as i64 + 300).rem_euclid(1440);
        format!("{:02}:{:02}", m / 60, m % 60)
    };
    let after_midnight = win.values().filter(|w| w.1 >= 19 * 60).count(); // 19 h after 05:00 = midnight
    let windows: Vec<f64> = win.values().map(|w| (w.1 - w.0) as f64 / 60.0).collect();
    let by_day_win: Vec<Value> = win.iter().rev().take(16).map(|(d, w)| json!([d, w.0, w.1, w.2, w.3])).collect();
    // Calories by part of day, last 7 eating days.
    let mut parts = [0.0f64; 3];
    for (ts, m) in &nut {
        let d = eat_day(*ts, tz);
        if d < add_days(&today, -7) {
            continue;
        }
        let mm = eat_min(*ts, tz) + 300;
        let k = m.get("energy").copied().unwrap_or(0.0);
        let i = if mm < 12 * 60 { 0 } else if mm < 20 * 60 { 1 } else { 2 };
        parts[i] += k;
    }
    let ptot = parts.iter().sum::<f64>().max(1.0);

    // ----- micronutrients, 7 days -----
    // Real = Apple's daily total minus what supplement entries in YAZIO carried (supplements are
    // counted only from the «Витамины и лекарства» section, when you mark them taken).
    let supp_days = crate::meds_svc::supp_by_day(conn, &add_days(&today, -6));
    let supp_avg = |m: &str| supp_days.values().map(|d| d.get(m).copied().unwrap_or(0.0)).sum::<f64>() / 7.0;
    let micro: Vec<Value> = MICRO
        .iter()
        .map(|(m, name, unit, _)| {
            let rda = crate::norms::norm(m, &country).unwrap_or(1.0);
            let s = series(conn, m, "max");
            let ei = crate::food_est::NUTS.iter().position(|(_, d)| d == m);
            let food_days: Vec<String> = (0..7)
                .map(|k| add_days(&today, -k))
                .filter(|d| kcal.contains_key(d) || est.get(d).map(|e| e.entries > 0).unwrap_or(false))
                .collect();
            let real_of = |d: &str| s.get(d).map(|v| (v - ei.map(|i| supp_of(d, i)).unwrap_or(0.0)).max(0.0));
            let real: Vec<f64> = food_days.iter().filter_map(|d| real_of(d)).collect();
            let avg = mean(&real);
            let with: Vec<f64> = food_days.iter().map(|d| real_of(d).unwrap_or(0.0) + ei.map(|i| est_of(d, i)).unwrap_or(0.0)).collect();
            let has_est = ei.map(|i| food_days.iter().any(|d| est_of(d, i) > 0.0)).unwrap_or(false);
            let avg_est = if has_est { mean(&with) } else { None };
            // Days that actually carry this nutrient (from YAZIO or the estimate).
            let covered = food_days
                .iter()
                .filter(|d| real_of(d).map(|v| v > 0.0).unwrap_or(false) || ei.map(|i| est_of(d, i) > 0.0).unwrap_or(false))
                .count();
            let dropped = ei.map(|i| food_days.iter().filter(|d| est.get(*d).map(|e| e.dropped.contains(&i)).unwrap_or(false)).count()).unwrap_or(0);
            let shown = avg_est.or(avg);
            let sv = supp_avg(m);
            let from_supp = if sv > 0.0 { Some((sv * 10.0).round() / 10.0) } else { None };
            let pct_all = if sv > 0.0 { Some(((shown.unwrap_or(0.0) + sv) / rda * 100.0).round()) } else { None };
            json!({"key": m, "name": name, "unit": unit, "rda": rda, "avg": avg.map(|a| (a * 10.0).round() / 10.0),
                   "pct": avg.map(|a| (a / rda * 100.0).round()), "days": real.len(),
                   "avgEst": avg_est.map(|a| (a * 10.0).round() / 10.0), "pctEst": avg_est.map(|a| (a / rda * 100.0).round()),
                   "covered": covered, "noData": covered < 3 || shown.unwrap_or(0.0) <= 0.0, "dropped": dropped,
                   "supp7": ei.map(|i| r1(food_days.iter().map(|d| supp_of(d, i)).sum::<f64>())),
                   "fromSupp": from_supp, "pctAll": pct_all })
        })
        .collect();

    let est7: Vec<&crate::food_est::DayEst> = (0..7).filter_map(|k| est.get(&add_days(&today, -k))).collect();
    let est_entries7: usize = est7.iter().map(|e| e.entries).sum();
    let est_done7: usize = est7.iter().map(|e| e.estimated).sum();
    let mut est_unknown7: Vec<String> = est7.iter().flat_map(|e| e.unknown.clone()).collect();
    est_unknown7.sort();
    est_unknown7.dedup();

    // ----- goal -----
    let weekly = match p.goal.as_str() {
        "lose" => -p.goal_rate,
        "gain" => p.goal_rate,
        _ => 0.0,
    };
    let eta_weeks = match (p.target_weight, weekly) {
        (Some(tw), w) if w != 0.0 && ((tw - w_now) / w) > 0.0 => Some(((tw - w_now) / w).ceil()),
        _ => None,
    };
    let monthly: Vec<Value> = {
        let mut by: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (d, v) in weight.range(add_days(&today, -200)..) {
            by.entry(d[0..7].to_string()).or_default().push(*v);
        }
        by.iter().map(|(m, v)| json!([m, median(v).map(r1)])).collect()
    };

    Ok(json!({
        "today": today,
        "tz": tz,
        "weight": r1(w_now),
        "targets": {"kcal": t.energy_kcal, "protein": t.protein_g, "carbs": t.carbs_g, "fat": t.fat_g, "fiber": t.fiber_g, "waterMl": t.water_ml,
                    "bmr": t.bmr_kcal, "neat": t.neat_kcal, "train": t.train_kcal, "goalKcal": t.goal_kcal, "trainMin": tmin.round()},
        "eaten": eaten_today,
        "goal": {"goal": p.goal, "rate": p.goal_rate, "targetWeight": p.target_weight, "etaWeeks": eta_weeks, "weekly": weekly,
                 "proteinPerKg": p.protein_g_per_kg, "proteinManual": p.protein_manual, "monthly": monthly},
        "days": days,
        "quality": {"fullDays": full30.len(), "fiber": avg_of(&fib).map(r1), "fiberWithEst": fiber_with_est.map(r1), "sugar": avg_of(&sug).map(|v| v.round()), "sugarPct": sug_pct,
                    "protein": avg_of(&prot).map(|v| v.round()), "proteinPerKg": avg_of(&prot).map(|v| r1(v / w_now)), "fatPct": fat_pct,
                    "satFat": avg_of(&sat).map(r1), "vegDays": veg_days, "top": top},
        "window": {"first": median(&firsts).map(hhmm), "last": median(&lasts).map(hhmm), "afterMidnight": after_midnight, "days": win.len(),
                   "hours": median(&windows).map(r1), "byDay": by_day_win,
                   "parts": [(parts[0] / ptot * 100.0).round(), (parts[1] / ptot * 100.0).round(), (parts[2] / ptot * 100.0).round()]},
        "micro": micro,
        "norms": {"country": country, "list": crate::norms::COUNTRIES.iter().map(|c| json!({"code": c.0, "name": c.1, "source": c.2})).collect::<Vec<_>>()},
        "estimate": {"entries7": est_entries7, "estimated7": est_done7, "unknown7": est_unknown7},
    }))
}

/// Meals of one eating day plus that day's totals.
pub fn day(conn: &Connection, date: &str) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let tz = p.tz_offset_s;
    let (items, nut) = entries(conn, &add_days(date, -1));
    let m = meals(&items, &nut, date, tz);
    let one = |metric: &str| -> Option<f64> {
        conn.query_row(
            "SELECT max(num) FROM hm WHERE metric = ?1 AND date = ?2",
            params![metric, date],
            |r| r.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten()
        .map(|v| v + crate::meds_svc::food_add(conn, metric).get(date).copied().unwrap_or(0.0))
        .or_else(|| crate::meds_svc::food_add(conn, metric).get(date).copied())
    };
    let total: f64 = m.iter().filter_map(|x| x["kcal"].as_f64()).sum();
    let caps = caps_for(&norms_country(conn));
    let est = crate::food_est::estimate(conn, date, tz, &caps);
    let de = est.get(date);
    let items: Vec<Value> = crate::food_est::entries(conn, date, tz)
        .into_iter()
        .filter(|(d, _)| d == date)
        .map(|(_, e)| json!({"time": clock(e.ts, tz), "name": e.name, "kcal": e.kcal.round(), "kind": e.kind, "parts": e.parts,
                             "fiber": r1(e.add[0]), "potassium": e.add[1].round(), "magnesium": e.add[2].round(), "iron": r1(e.add[4])}))
        .collect();
    let late: f64 = m
        .iter()
        .filter(|x| {
            let t = x["ts"].as_i64().unwrap_or(0);
            eat_min(t, tz) + 300 >= 20 * 60
        })
        .filter_map(|x| x["kcal"].as_f64())
        .sum();
    Ok(json!({
        "date": date,
        "meals": m,
        "kcal": one("dietary_energy_total").map(|v| v.round()),
        "protein": one("dietary_protein_total").map(|v| v.round()),
        "carbs": one("dietary_carbohydrates_total").map(|v| v.round()),
        "fat": one("dietary_fat_total_total").map(|v| v.round()),
        "fiber": one("dietary_fiber_total").map(r1),
        "fiberEst": de.map(|e| r1(e.add[0])),
        "estimated": de.map(|e| json!({"entries": e.entries, "estimated": e.estimated, "unknown": e.unknown})),
        "items": items,
        "sugar": one("dietary_sugar_total").map(|v| v.round()),
        "waterMl": one("dietary_water_total").map(|v| v.round()),
        "latePct": if total > 0.0 { Some((late / total * 100.0).round()) } else { None },
    }))
}
