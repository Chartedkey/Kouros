//! Where each kind of health data comes from (0.14). freddy delivers rows from every source the
//! athlete connected there (Apple Health, Garmin, Suunto, Fitbit via Google Health, …) and Kouros
//! keeps them all. Every screen reads through the temporary view `hm`, which shows Apple's names
//! and, for each data group, only the rows of the chosen source — for the dates it was chosen.
//! Other sources' metrics are renamed to Apple's names (auto-matched by name, the athlete can pick
//! another one by hand) and converted to the same units. Nothing is deleted, so switching back is
//! instant.

use crate::db;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const APPLE: &str = "APPLE_HEALTH";

/// Known sources (freddy provider code, name). Any other provider found in the data works too.
pub const PROVIDERS: [(&str, &str); 4] = [(APPLE, "Apple Health"), ("GARMIN", "Garmin"), ("SUUNTO", "Suunto"), ("GOOGLE_HEALTH", "Fitbit (Google Health)")];

/// Data groups: key, name, Apple metrics in it, whether other sources can provide it.
pub const GROUPS: [(&str, &str, &[&str], bool); 7] = [
    ("sleep", "Сон", &["sleep_analysis_core_seconds", "sleep_analysis_deep_seconds", "sleep_analysis_rem_seconds", "sleep_analysis_awake_seconds", "sleep_analysis_raw"], true),
    ("hrv", "HRV", &["hrv_sdnn_raw", "hrv_rmssd_merged_avg", "hrv_rmssd_avg", "hrv_sdnn_merged_avg"], true),
    ("rhr", "Пульс покоя", &["resting_heart_rate_merged_avg"], true),
    ("vitals", "Дыхание, SpO₂ и температура", &["respiratory_rate_merged_avg", "sleeping_breathing_disturbances", "blood_oxygen", "blood_oxygen_avg", "wrist_temp"], true),
    ("activity", "Шаги, калории и VO₂max", &["steps_total", "active_energy_total", "basal_energy_total", "vo2_max"], true),
    ("body", "Вес и состав тела", &["body_mass", "body_fat_pct", "lean_body_mass"], true),
    ("pulse", "Пульс днём и стресс", &["heart_rate_raw", "steps_raw", "walking_hr_avg_merged_value"], false),
];

/// Unit family for conversion.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Dur,
    Plain,
    Mass,
    Temp,
    Energy,
}

/// What another source's metric must look like to stand in for an Apple metric.
struct Target {
    canon: &'static str,
    group: &'static str,
    label: &'static str,
    want: &'static [&'static str],
    any: &'static [&'static str],
    avoid: &'static [&'static str],
    prefer: &'static [&'static str],
    kind: Kind,
}

const SLEEP_AVOID: &[&str] = &["pct", "percent", "score", "ratio", "start", "end", "count", "goal", "need", "nap"];
const TARGETS: [Target; 14] = [
    Target { canon: "sleep_analysis_deep_seconds", group: "sleep", label: "Глубокий сон", want: &["deep"], any: &["sleep"], avoid: SLEEP_AVOID, prefer: &["seconds", "duration"], kind: Kind::Dur },
    Target { canon: "sleep_analysis_rem_seconds", group: "sleep", label: "Быстрый сон (REM)", want: &["rem"], any: &["sleep"], avoid: SLEEP_AVOID, prefer: &["seconds", "duration"], kind: Kind::Dur },
    Target { canon: "sleep_analysis_core_seconds", group: "sleep", label: "Лёгкий сон", want: &["sleep"], any: &["light", "core"], avoid: SLEEP_AVOID, prefer: &["seconds", "duration"], kind: Kind::Dur },
    Target { canon: "sleep_analysis_awake_seconds", group: "sleep", label: "Пробуждения", want: &["awake"], any: &[], avoid: SLEEP_AVOID, prefer: &["sleep", "seconds", "duration"], kind: Kind::Dur },
    Target { canon: "hrv_rmssd_merged_avg", group: "hrv", label: "HRV за ночь", want: &["hrv"], any: &[], avoid: &["status", "baseline", "balance", "weekly", "max", "min", "low", "high", "score"], prefer: &["night", "avg", "rmssd", "last", "sleep"], kind: Kind::Plain },
    Target { canon: "resting_heart_rate_merged_avg", group: "rhr", label: "Пульс покоя", want: &[], any: &["resting_heart", "resting_hr", "restinghr", "restingheart", "rhr"], avoid: &["max", "variab", "zone"], prefer: &["avg"], kind: Kind::Plain },
    Target { canon: "respiratory_rate_merged_avg", group: "vitals", label: "Дыхание во сне", want: &[], any: &["respir", "breathing_rate", "breath_rate"], avoid: &["max", "min", "low", "high"], prefer: &["sleep", "avg"], kind: Kind::Plain },
    Target { canon: "blood_oxygen_avg", group: "vitals", label: "SpO₂", want: &[], any: &["spo2", "oxygen", "pulse_ox", "pulseox"], avoid: &["max", "min", "low", "high"], prefer: &["avg", "sleep"], kind: Kind::Plain },
    Target { canon: "wrist_temp", group: "vitals", label: "Температура тела", want: &["temp"], any: &[], avoid: &["air", "ambient", "water", "room", "max", "min"], prefer: &["skin", "wrist", "body", "deviation", "sleep"], kind: Kind::Temp },
    Target { canon: "steps_total", group: "activity", label: "Шаги", want: &["steps"], any: &[], avoid: &["goal", "floor", "cadence"], prefer: &["total", "daily"], kind: Kind::Plain },
    Target { canon: "active_energy_total", group: "activity", label: "Активные калории", want: &["active"], any: &["calor", "energy", "kcal"], avoid: &["goal", "minutes", "time"], prefer: &["total"], kind: Kind::Energy },
    Target { canon: "vo2_max", group: "activity", label: "VO₂max", want: &["vo2"], any: &[], avoid: &[], prefer: &["max"], kind: Kind::Plain },
    Target { canon: "body_mass", group: "body", label: "Вес", want: &[], any: &["weight", "body_mass"], avoid: &["lean", "goal", "bone", "muscle", "water", "fat"], prefer: &[], kind: Kind::Mass },
    Target { canon: "body_fat_pct", group: "body", label: "Процент жира", want: &[], any: &["body_fat", "fat_pct", "fat_percent", "fat_ratio", "bodyfat"], avoid: &["mass", "kg"], prefer: &["pct", "percent"], kind: Kind::Plain },
];
const TARGET_LEAN: Target = Target { canon: "lean_body_mass", group: "body", label: "Сухая масса", want: &[], any: &["lean", "muscle_mass", "fat_free"], avoid: &["pct", "percent"], prefer: &[], kind: Kind::Mass };

fn targets() -> impl Iterator<Item = &'static Target> {
    TARGETS.iter().chain(std::iter::once(&TARGET_LEAN))
}

fn is_apple(p: &str) -> bool {
    p.is_empty() || p.to_uppercase().starts_with("APPLE")
}

pub fn provider_name(p: &str) -> String {
    PROVIDERS.iter().find(|x| x.0.eq_ignore_ascii_case(p)).map(|x| x.1.to_string()).unwrap_or_else(|| {
        let mut s = p.replace('_', " ").to_lowercase();
        if let Some(f) = s.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        s
    })
}

/// A metric another source has in Kouros (from the stored rows).
#[derive(Clone)]
pub struct Cat {
    pub provider: String,
    pub metric: String,
    pub unit: String,
    pub count: i64,
    pub first: String,
    pub last: String,
    pub avg: Option<f64>,
}

pub fn catalog(conn: &Connection) -> Vec<Cat> {
    let Ok(mut st) = conn.prepare(
        "SELECT provider, metric, coalesce(max(unit), ''), count(*), min(date), max(date), avg(num) FROM health_metrics
         WHERE provider <> '' AND upper(provider) NOT LIKE 'APPLE%' GROUP BY provider, metric",
    ) else {
        return Vec::new();
    };
    let v = st
        .query_map([], |r| {
            Ok(Cat { provider: r.get(0)?, metric: r.get(1)?, unit: r.get(2)?, count: r.get(3)?, first: r.get(4)?, last: r.get(5)?, avg: r.get(6)? })
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    v
}

fn score(t: &Target, name: &str) -> Option<i64> {
    let n = name.to_lowercase();
    if n.ends_with("_raw") || n.contains("sample") || n.contains("_entry") || n.starts_with("workout") || n.starts_with("activity_") {
        return None;
    }
    if !t.want.iter().all(|w| n.contains(w)) || (!t.any.is_empty() && !t.any.iter().any(|w| n.contains(w))) || t.avoid.iter().any(|w| n.contains(w)) {
        return None;
    }
    Some(200 - n.len() as i64 + 15 * t.prefer.iter().filter(|w| n.contains(*w)).count() as i64)
}

/// (a, b, temperature-in-°F) so that value = a·num + b.
fn convert(kind: Kind, unit: &str, avg: Option<f64>) -> (f64, f64, bool) {
    let u = unit.to_lowercase();
    match kind {
        Kind::Dur => match u.as_str() {
            "min" | "mins" | "minute" | "minutes" => (60.0, 0.0, false),
            "h" | "hr" | "hrs" | "hour" | "hours" => (3600.0, 0.0, false),
            "ms" => (0.001, 0.0, false),
            "s" | "sec" | "secs" | "seconds" => (1.0, 0.0, false),
            // No unit: a night of light sleep in minutes is < 1000, in seconds > 1000.
            _ => if avg.map(|a| a > 0.0 && a < 24.0).unwrap_or(false) { (3600.0, 0.0, false) } else if avg.map(|a| a < 1000.0).unwrap_or(false) { (60.0, 0.0, false) } else { (1.0, 0.0, false) },
        },
        Kind::Mass => match u.as_str() {
            "lb" | "lbs" | "pound" | "pounds" => (0.453592, 0.0, false),
            "g" => (0.001, 0.0, false),
            _ => (1.0, 0.0, false),
        },
        Kind::Energy => if u == "kj" { (1.0 / 4.184, 0.0, false) } else { (1.0, 0.0, false) },
        Kind::Temp => (1.0, 0.0, u.contains('f')),
        Kind::Plain => (1.0, 0.0, false),
    }
}

fn settings(conn: &Connection) -> Value {
    db::get_setting(conn, "data_sources").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}

/// Segments of a group: (provider, from) in time order; the first one starts at the beginning.
pub fn segments(cfg: &Value, group: &str) -> Vec<(String, String)> {
    let v: Vec<(String, String)> = cfg["groups"][group]["segs"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| Some((s["p"].as_str()?.to_string(), s["from"].as_str().unwrap_or("").to_string()))).collect())
        .unwrap_or_default();
    if v.is_empty() { vec![(APPLE.to_string(), String::new())] } else { v }
}

/// The chosen metric of `provider` for each Apple metric: manual pick first, else the best name match.
fn mapping(cfg: &Value, cat: &[Cat], provider: &str) -> BTreeMap<&'static str, (Cat, bool)> {
    let mut out = BTreeMap::new();
    let own: Vec<&Cat> = cat.iter().filter(|c| c.provider.eq_ignore_ascii_case(provider)).collect();
    for t in targets() {
        let manual = cfg["map"][provider][t.canon].as_str();
        if manual == Some("") {
            continue; // switched off by hand
        }
        if let Some(m) = manual {
            if let Some(c) = own.iter().find(|c| c.metric == m) {
                out.insert(t.canon, ((*c).clone(), true));
                continue;
            }
        }
        let best = own.iter().filter_map(|c| score(t, &c.metric).map(|s| (s + (c.count as f64).ln().round() as i64, *c))).max_by_key(|x| x.0);
        if let Some((_, c)) = best {
            out.insert(t.canon, (c.clone(), false));
        }
    }
    out
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn date_cond(segs: &[(String, String)], provider: &str) -> Option<String> {
    let mut parts = Vec::new();
    for (i, (p, from)) in segs.iter().enumerate() {
        if !p.eq_ignore_ascii_case(provider) && !(is_apple(p) && is_apple(provider)) {
            continue;
        }
        let to = segs.get(i + 1).map(|x| x.1.clone());
        let mut c = Vec::new();
        if !from.is_empty() {
            c.push(format!("date >= {}", q(from)));
        }
        if let Some(t) = to {
            c.push(format!("date < {}", q(&t)));
        }
        parts.push(if c.is_empty() { "1".to_string() } else { format!("({})", c.join(" AND ")) });
    }
    if parts.is_empty() { None } else { Some(parts.join(" OR ")) }
}

/// (Re)creates the view `hm` every reader uses. Called when the database is opened and after a change.
pub fn install_view(conn: &Connection) -> Result<(), String> {
    let cfg = settings(conn);
    let cat = catalog(conn);
    let mut apple_where = vec!["(provider = '' OR upper(provider) LIKE 'APPLE%')".to_string()];
    let mut unions = Vec::new();
    for (g, _, metrics, _) in GROUPS.iter() {
        let segs = segments(&cfg, g);
        if segs.len() == 1 && is_apple(&segs[0].0) {
            continue;
        }
        let list = metrics.iter().map(|m| q(m)).collect::<Vec<_>>().join(",");
        match date_cond(&segs, APPLE) {
            Some(c) => apple_where.push(format!("NOT (metric IN ({list}) AND NOT ({c}))")),
            None => apple_where.push(format!("metric NOT IN ({list})")),
        }
        let mut provs: Vec<String> = segs.iter().map(|s| s.0.clone()).filter(|p| !is_apple(p)).collect();
        provs.sort();
        provs.dedup();
        for p in provs {
            let Some(cond) = date_cond(&segs, &p) else { continue };
            let map = mapping(&cfg, &cat, &p);
            for t in targets().filter(|t| t.group == *g) {
                let Some((c, _)) = map.get(t.canon) else { continue };
                let (a, b, fahr) = convert(t.kind, &c.unit, c.avg);
                let num = if fahr { "(CASE WHEN num > 50 THEN (num - 32) * 5.0 / 9 ELSE num * 5.0 / 9 END)".to_string() } else { format!("(num * {a} + {b})") };
                unions.push(format!(
                    "SELECT {} AS metric, date, ts, provider, device, source_id, source, {num} AS num, txt, unit FROM health_metrics WHERE provider = {} AND metric = {} AND ({cond})",
                    q(t.canon), q(&c.provider), q(&c.metric)
                ));
            }
        }
    }
    let mut sql = format!("SELECT metric, date, ts, provider, device, source_id, source, num, txt, unit FROM health_metrics WHERE {}", apple_where.join(" AND "));
    for u in unions {
        sql.push_str("\nUNION ALL\n");
        sql.push_str(&u);
    }
    conn.execute_batch(&format!("DROP VIEW IF EXISTS temp.hm; CREATE TEMP VIEW hm AS {sql};")).map_err(|e| e.to_string())
}

/// Start of the current source if it replaced another one only from that date: the personal norm
/// for this group is then built from that date on ("норма набирается").
pub fn cut(conn: &Connection, group: &str) -> Option<String> {
    let segs = segments(&settings(conn), group);
    let last = segs.last()?;
    if segs.len() > 1 && !last.1.is_empty() { Some(last.1.clone()) } else { None }
}

pub fn current(conn: &Connection, group: &str) -> String {
    segments(&settings(conn), group).last().map(|x| x.0.clone()).unwrap_or(APPLE.to_string())
}

pub fn current_is_apple(conn: &Connection, group: &str) -> bool {
    segments(&settings(conn), group).last().map(|x| is_apple(&x.0)).unwrap_or(true)
}

/// All the cuts at once (group → date).
pub fn cuts(conn: &Connection) -> BTreeMap<String, String> {
    GROUPS.iter().filter_map(|g| cut(conn, g.0).map(|c| (g.0.to_string(), c))).collect()
}

/// Source switches for chart marks: (group, date, from provider, to provider).
pub fn switches(conn: &Connection) -> Vec<Value> {
    let cfg = settings(conn);
    let mut out = Vec::new();
    for (g, name, _, _) in GROUPS.iter() {
        let segs = segments(&cfg, g);
        for w in segs.windows(2) {
            if !w[1].1.is_empty() {
                out.push(json!({"group": g, "name": name, "date": w[1].1, "from": provider_name(&w[0].0), "to": provider_name(&w[1].0)}));
            }
        }
    }
    out
}

fn warning(group: &str, from: &str, to: &str) -> String {
    let (f, t) = (provider_name(from), provider_name(to));
    match group {
        "hrv" => format!("{t} и {f} меряют HRV по-разному (разные методы и время замера) — цифры будут другого уровня, часто заметно выше или ниже. Норма HRV пересчитается заново."),
        "rhr" => format!("{t} считает пульс покоя по-своему — разница обычно 2–5 ударов. Норма пересчитается заново."),
        "sleep" => format!("{t} определяет фазы сна по-своему: глубокого и быстрого сна может стать больше или меньше. Подробная гипнограмма (сон по минутам) пока есть только от Apple — от {t} будут итоги ночи."),
        "vitals" => format!("Дыхание, SpO₂ и температуру {t} меряет своими датчиками — уровни могут сдвинуться. Нормы пересчитаются."),
        "activity" => format!("Шаги и калории у {t} и {f} считаются по-разному, особенно калории. Цели питания зависят от калорий тренировок — они могут немного сдвинуться."),
        "body" => format!("Весы {t} и {f} могут отличаться на несколько сотен граммов и заметно — по проценту жира."),
        _ => format!("Данные {t} и {f} могут отличаться."),
    }
}

/// Everything the settings card needs.
pub fn state(conn: &Connection) -> Value {
    let cfg = settings(conn);
    let cat = catalog(conn);
    let mut provs: Vec<String> = cat.iter().map(|c| c.provider.clone()).collect();
    provs.sort();
    provs.dedup();
    let maps: BTreeMap<String, BTreeMap<&str, (Cat, bool)>> = provs.iter().map(|p| (p.clone(), mapping(&cfg, &cat, p))).collect();
    let today = crate::engine_svc::today(&crate::engine_svc::profile(conn));
    let from60 = crate::engine::add_days(&today, -60);
    let groups: Vec<Value> = GROUPS
        .iter()
        .map(|(g, name, metrics, open)| {
            let segs = segments(&cfg, g);
            let cur = segs.last().map(|x| x.0.clone()).unwrap_or(APPLE.to_string());
            let options: Vec<Value> = std::iter::once(json!({"provider": APPLE, "name": "Apple Health", "matched": metrics.len(), "of": metrics.len()}))
                .chain(provs.iter().filter(|_| *open).map(|p| {
                    let m = &maps[p];
                    let of = targets().filter(|t| t.group == *g).count();
                    let matched = targets().filter(|t| t.group == *g && m.contains_key(t.canon)).count();
                    json!({"provider": p, "name": provider_name(p), "matched": matched, "of": of})
                }))
                .collect();
            // Who writes this group into Apple Health (a Garmin can write there too).
            let list = metrics.iter().map(|m| q(m)).collect::<Vec<_>>().join(",");
            let writers: Vec<Value> = conn
                .prepare(&format!(
                    "SELECT coalesce(nullif(source, ''), device), count(*) FROM health_metrics WHERE (provider = '' OR upper(provider) LIKE 'APPLE%')
                     AND metric IN ({list}) AND date >= ?1 AND coalesce(nullif(source, ''), device) NOT LIKE '%merged%' GROUP BY 1 ORDER BY 2 DESC LIMIT 6"
                ))
                .ok()
                .map(|mut st| {
                    let v: Vec<Value> = st.query_map(params![from60], |r| Ok(json!({"name": r.get::<_, String>(0)?, "count": r.get::<_, i64>(1)?}))).map(|r| r.flatten().collect()).unwrap_or_default();
                    v
                })
                .unwrap_or_default();
            let fields: Vec<Value> = if is_apple(&cur) {
                Vec::new()
            } else {
                let own: Vec<&Cat> = cat.iter().filter(|c| c.provider.eq_ignore_ascii_case(&cur)).collect();
                targets()
                    .filter(|t| t.group == *g)
                    .map(|t| {
                        let m = maps.get(&cur).and_then(|m| m.get(t.canon));
                        let mut cands: Vec<(i64, &Cat)> = own.iter().map(|c| (score(t, &c.metric).unwrap_or(-1000), *c)).collect();
                        cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.metric.cmp(&b.1.metric)));
                        json!({"canon": t.canon, "label": t.label, "metric": m.map(|x| x.0.metric.clone()), "manual": m.map(|x| x.1).unwrap_or(false),
                               "unit": m.map(|x| x.0.unit.clone()), "last": m.map(|x| x.0.last.clone()), "count": m.map(|x| x.0.count),
                               "candidates": cands.iter().map(|(_, c)| json!({"metric": c.metric, "unit": c.unit, "count": c.count})).collect::<Vec<_>>()})
                    })
                    .collect()
            };
            json!({
                "key": g, "name": name, "open": open, "current": cur, "currentName": provider_name(&cur),
                "segs": segs.iter().map(|(p, f)| json!({"provider": p, "name": provider_name(p), "from": f})).collect::<Vec<_>>(),
                "cut": cut(conn, g), "options": options, "writers": writers, "fields": fields,
            })
        })
        .collect();
    json!({
        "groups": groups,
        "known": PROVIDERS.iter().map(|(p, n)| json!({"provider": p, "name": n, "connected": is_apple(p) || provs.iter().any(|x| x.eq_ignore_ascii_case(p))})).collect::<Vec<_>>(),
        "providers": provs.iter().map(|p| {
            let c: Vec<&Cat> = cat.iter().filter(|c| &c.provider == p).collect();
            json!({"provider": p, "name": provider_name(p), "metrics": c.len(), "first": c.iter().map(|x| x.first.clone()).min(), "last": c.iter().map(|x| x.last.clone()).max()})
        }).collect::<Vec<_>>(),
        "today": today,
    })
}

/// Text of the warning before a switch (the UI shows it with the two choices).
pub fn preview(conn: &Connection, group: &str, provider: &str) -> Value {
    let cfg = settings(conn);
    let segs = segments(&cfg, group);
    let cur = segs.last().map(|x| x.0.clone()).unwrap_or(APPLE.to_string());
    let first: Option<String> = if is_apple(provider) {
        None
    } else {
        let cat = catalog(conn);
        let map = mapping(&cfg, &cat, provider);
        targets().filter(|t| t.group == group).filter_map(|t| map.get(t.canon).map(|x| x.0.first.clone())).min()
    };
    let has_apple_history = is_apple(provider);
    json!({"text": warning(group, &cur, provider), "historyFrom": first, "appleHistory": has_apple_history})
}

/// Switches a group. `mode`: "keep" — the new source from today, earlier days stay as they were;
/// "replace" — the new source for the whole history (where it has data).
pub fn set(conn: &Connection, group: &str, provider: &str, mode: &str) -> Result<(), String> {
    if !GROUPS.iter().any(|g| g.0 == group) {
        return Err("Неизвестная группа данных".into());
    }
    let mut cfg = settings(conn);
    let mut segs = segments(&cfg, group);
    let today = crate::engine_svc::today(&crate::engine_svc::profile(conn));
    if mode == "replace" {
        segs = vec![(provider.to_string(), String::new())];
    } else if segs.last().map(|x| !x.0.eq_ignore_ascii_case(provider)).unwrap_or(true) {
        segs.retain(|s| s.1 < today);
        if segs.is_empty() {
            segs.push((provider.to_string(), String::new()));
        } else {
            segs.push((provider.to_string(), today.clone()));
        }
    }
    // Merge neighbours with the same source.
    let mut merged: Vec<(String, String)> = Vec::new();
    for s in segs {
        if merged.last().map(|m| m.0.eq_ignore_ascii_case(&s.0)).unwrap_or(false) {
            continue;
        }
        merged.push(s);
    }
    if let Some(f) = merged.first_mut() {
        f.1 = String::new();
    }
    cfg["groups"][group] = json!({"segs": merged.iter().map(|(p, f)| json!({"p": p, "from": f})).collect::<Vec<_>>()});
    db::set_setting(conn, "data_sources", &cfg.to_string())?;
    // Frozen readiness is no longer right: today's always, the whole history when it was replaced.
    if mode == "replace" {
        let _ = conn.execute("DELETE FROM readiness_log", []);
    } else {
        let _ = conn.execute("DELETE FROM readiness_log WHERE date = ?1", params![today]);
    }
    install_view(conn)
}

/// Manual pick of another source's metric for an Apple metric (`metric` None = back to automatic,
/// Some("") = don't use).
pub fn map_set(conn: &Connection, provider: &str, canon: &str, metric: Option<&str>) -> Result<(), String> {
    let mut cfg = settings(conn);
    if !cfg["map"].is_object() {
        cfg["map"] = json!({});
    }
    if !cfg["map"][provider].is_object() {
        cfg["map"][provider] = json!({});
    }
    match metric {
        Some(m) => cfg["map"][provider][canon] = json!(m),
        None => {
            if let Some(o) = cfg["map"][provider].as_object_mut() {
                o.remove(canon);
            }
        }
    }
    db::set_setting(conn, "data_sources", &cfg.to_string())?;
    install_view(conn)
}
