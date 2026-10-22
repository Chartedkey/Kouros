//! «Тренер», part 2 (0.15): the weekly template (from the coach onboarding), the plan for the
//! coming days with code checks (week headroom, no two hard days in a row, phases with a lighter
//! 4th week), daily proposals to swap a workout when the body or the gates say so (never applied
//! without consent unless the athlete chose «Сразу»), done/missed marks from Strava, the history of
//! changes and proposals parsed from Claude's answers ([ПЛАН] lines).

use crate::engine::{add_days, day_num, day_of, median};
use crate::{coach_svc, db, engine_svc, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Kind: key, title, sport group, hard day, readiness tier it needs (index in `daily::TIERS`).
pub const KINDS: [(&str, &str, &str, bool, usize); 13] = [
    ("rest", "Отдых", "", false, 0),
    ("walk", "Долгая прогулка", "walk", false, 1),
    ("mobility", "Растяжка и мобилити", "mobility", false, 1),
    ("easy", "Лёгкий бег", "run", false, 3),
    ("ofp", "Укрепляющие упражнения", "strength", false, 3),
    ("bike", "Велосипед, спокойно", "ride", false, 3),
    ("long", "Длительный бег", "run", true, 4),
    ("gym", "Силовая в зале", "strength", false, 4),
    ("tempo", "Темповый бег", "run", true, 5),
    ("intervals", "Интервалы", "run", true, 6),
    ("upper", "Верх тела и кор", "strength", false, 3),
    ("walkrun", "Бег с ходьбой", "run", false, 2),
    ("test", "Контрольный тест 5 км", "run", true, 5),
];

fn is_run(k: &str) -> bool {
    matches!(k, "easy" | "long" | "tempo" | "intervals" | "walkrun" | "test")
}

fn kind(k: &str) -> (&'static str, &'static str, &'static str, bool, usize) {
    KINDS.iter().copied().find(|x| x.0 == k).unwrap_or(KINDS[0])
}

const WD: [&str; 7] = ["пн", "вт", "ср", "чт", "пт", "сб", "вс"];
const WD_FULL: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
const MONTHS: [&str; 12] = ["янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub min: f64,
}

fn items_json(v: &[Item]) -> Value {
    json!(v.iter().map(|i| json!({"kind": i.kind, "min": i.min})).collect::<Vec<_>>())
}

fn items_of(v: &Value) -> Vec<Item> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| Some(Item { kind: x["kind"].as_str()?.to_string(), min: x["min"].as_f64().unwrap_or(0.0) })).collect())
        .unwrap_or_default()
}

fn hard(v: &[Item]) -> bool {
    v.iter().any(|i| kind(&i.kind).3)
}

fn need_tier(v: &[Item]) -> usize {
    v.iter().map(|i| kind(&i.kind).4).max().unwrap_or(0)
}

fn r5(x: f64) -> f64 {
    ((x / 5.0).round() * 5.0).max(0.0)
}

/// Monday index of a date (0 = Monday).
fn wday(d: &str) -> usize {
    // 1970-01-01 was a Thursday.
    ((day_num(d).unwrap_or(0) + 3).rem_euclid(7)) as usize
}

fn monday(d: &str) -> String {
    add_days(d, -(wday(d) as i64))
}

pub fn date_ru(d: &str) -> String {
    let m: usize = d.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(1);
    let dd: i64 = d.get(8..10).and_then(|x| x.parse().ok()).unwrap_or(1);
    format!("{} {} {}", WD[wday(d)], dd, MONTHS[(m.max(1) - 1).min(11)])
}

pub fn items_text(v: &[Item]) -> String {
    if v.is_empty() {
        return "Отдых".into();
    }
    v.iter()
        .map(|i| {
            let k = kind(&i.kind);
            if i.kind == "rest" { k.1.to_string() } else { format!("{} {} мин", k.1, i.min.round()) }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

// ---------- template ----------

fn get_json(conn: &Connection, key: &str) -> Value {
    db::get_setting(conn, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

/// Builds the usual week from the onboarding answers.
pub fn build_template(cp: &Value) -> Vec<Vec<Item>> {
    let wk = &cp["week"];
    let cnt = |k: &str, d: f64| wk[k].as_f64().unwrap_or(d).clamp(0.0, 7.0) as usize;
    let has_week = wk.is_object();
    let r = if has_week { cnt("run", 0.0) } else { 3 };
    let g = if has_week { cnt("gym", 0.0) } else { 1 };
    let o = if has_week { cnt("ofp", 0.0) } else { 1 };
    let b = if has_week { cnt("bike", 0.0) } else { 0 };
    let days: Vec<usize> = cp["days"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).filter_map(|x| WD_FULL.iter().position(|w| *w == x)).collect())
        .filter(|v: &Vec<usize>| !v.is_empty())
        .unwrap_or_else(|| (0..7).collect());
    let t = cp["time"].as_str().and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(60.0);
    let mode = cp["mode"].as_str().unwrap_or("maintain");
    let goals: Vec<&str> = cp["goals"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let r = r.min(days.len());
    let mut week: Vec<Vec<Item>> = vec![Vec::new(); 7];
    let dist = |a: usize, b: usize| {
        let d = (a as i64 - b as i64).rem_euclid(7);
        d.min(7 - d) as usize
    };
    let it = |k: &str, m: f64| Item { kind: k.into(), min: r5(m) };
    let mut runs: Vec<usize> = Vec::new();
    let mut hards: Vec<usize> = Vec::new();
    // Long run at the weekend.
    if r >= 2 {
        let l = [6, 5].iter().copied().find(|d| days.contains(d)).unwrap_or(*days.last().unwrap_or(&6));
        let lm = if goals.contains(&"mar") { (t * 1.8).clamp(80.0, 150.0) } else if mode == "winter" { (t * 1.4).clamp(60.0, 75.0) } else { (t * 1.5).clamp(70.0, 120.0) };
        week[l].push(it("long", lm));
        runs.push(l);
        hards.push(l);
    }
    // Quality: away from the long run and from each other.
    let q = match mode {
        "return" => 0,
        "build" if r >= 5 => 2,
        _ if r >= 3 => 1,
        _ => 0,
    };
    let first = if goals.contains(&"k10") && mode != "winter" { "intervals" } else { "tempo" };
    for n in 0..q {
        let cand = days.iter().copied().filter(|d| !runs.contains(d) && hards.iter().all(|h| dist(*d, *h) >= 2)).max_by_key(|d| {
            let m = hards.iter().map(|h| dist(*d, *h)).min().unwrap_or(7);
            m * 10 + if *d == 1 || *d == 3 { 5 } else { 0 }
        });
        if let Some(d) = cand {
            let k = if n == 0 { first } else if first == "tempo" { "intervals" } else { "tempo" };
            week[d].push(it(k, t.max(40.0)));
            runs.push(d);
            hards.push(d);
        }
    }
    // Easy runs spread out.
    while runs.len() < r {
        let cand = days.iter().copied().filter(|d| !runs.contains(d)).max_by_key(|d| runs.iter().map(|x| dist(*d, *x)).min().unwrap_or(7) * 10 + (7 - *d));
        let Some(d) = cand else { break };
        week[d].push(it("easy", t.max(30.0)));
        runs.push(d);
    }
    // Gym: a day without a run if possible, not right before a hard day.
    let mut strength: Vec<usize> = Vec::new();
    for _ in 0..g {
        let score = |d: usize| -> i64 {
            let mut s = 0;
            if week[d].is_empty() {
                s += 20;
            }
            if hards.contains(&((d + 1) % 7)) {
                s -= 15;
            }
            if hards.contains(&d) {
                s -= 30;
            }
            s + strength.iter().map(|x| dist(d, *x) as i64).min().unwrap_or(7)
        };
        let Some(d) = days.iter().copied().filter(|d| !strength.contains(d)).max_by_key(|d| score(*d)) else { break };
        week[d].push(it("gym", t.max(40.0)));
        strength.push(d);
    }
    for _ in 0..o {
        let score = |d: usize| -> i64 { (if hards.contains(&d) { -30 } else { 0 }) + if week[d].iter().any(|i| i.kind == "gym" || i.kind == "ofp") { -40 } else { 0 } + if week[d].is_empty() { 5 } else { 0 } };
        let Some(d) = days.iter().copied().max_by_key(|d| score(*d)) else { break };
        if score(d) <= -40 {
            break;
        }
        week[d].push(it("ofp", t.min(30.0)));
    }
    for _ in 0..b {
        let Some(d) = days.iter().copied().find(|d| week[*d].is_empty()) else { break };
        week[d].push(it("bike", t));
    }
    week
}

fn template(conn: &Connection) -> (Vec<Vec<Item>>, bool, String) {
    let v = get_json(conn, "plan_template");
    let today = engine_svc::today(&engine_svc::profile(conn));
    let start = v["start"].as_str().map(String::from).unwrap_or_else(|| monday(&today));
    if let Some(a) = v["days"].as_array().filter(|a| a.len() == 7) {
        return (a.iter().map(items_of).collect(), v["auto"].as_bool().unwrap_or(false), start);
    }
    (build_template(&coach_svc::profile(conn)), true, start)
}

fn save_template(conn: &Connection, days: &[Vec<Item>], auto: bool, start: &str) -> Result<(), String> {
    let v = json!({"days": days.iter().map(|d| items_json(d)).collect::<Vec<_>>(), "auto": auto, "start": start});
    db::set_setting(conn, "plan_template", &v.to_string())
}

// ---------- phases and loads ----------

fn mode(conn: &Connection) -> String {
    coach_svc::profile(conn)["mode"].as_str().unwrap_or("maintain").to_string()
}

/// (week of the 4-week block 1..4, minutes factor, deload).
fn phase(start: &str, d: &str, mode: &str) -> (i64, f64, bool) {
    let w = ((day_num(&monday(d)).unwrap_or(0) - day_num(start).unwrap_or(0)) / 7).max(0);
    let k = w % 4 + 1;
    let f = match (mode, k) {
        (_, 4) => if mode == "build" { 0.7 } else { 0.75 },
        ("build", 2) => 1.05,
        ("build", 3) => 1.1,
        ("return", 1) => 0.8,
        ("return", 2) => 0.9,
        _ => 1.0,
    };
    (w / 4 + 1, f, k == 4)
}

pub fn phase_name(mode: &str) -> &'static str {
    match mode {
        "build" => "Набор объёма",
        "winter" => "Зимовка",
        "return" => "Возвращение",
        _ => "Поддержание формы",
    }
}

/// Typical load per minute by plan kind, from your own workouts of the last 90 days.
fn rates(acts: &[engine_svc::Cached], today: &str, tz: i64) -> BTreeMap<&'static str, f64> {
    let from = add_days(today, -90);
    let mut by: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for a in acts {
        if day_of(a.start_utc, tz) < from || a.duration_s < 600.0 || a.load <= 0.0 {
            continue;
        }
        let g = sport_group(&a.sport);
        if !g.is_empty() {
            by.entry(g).or_default().push(a.load / (a.duration_s / 60.0));
        }
    }
    let m = |g: &str, d: f64| by.get(g).and_then(|v| median(v)).unwrap_or(d);
    let run = m("run", 1.2);
    BTreeMap::from([
        ("easy", run), ("long", run * 1.05), ("tempo", run * 1.35), ("intervals", run * 1.5),
        ("gym", m("strength", 0.6)), ("ofp", m("strength", 0.6) * 0.7), ("bike", m("ride", 0.9)), ("walk", m("walk", 0.4)), ("mobility", 0.25), ("rest", 0.0),
        ("upper", m("strength", 0.6) * 0.8), ("walkrun", run * 0.6), ("test", run * 1.5),
    ])
}

fn sport_group(s: &str) -> &'static str {
    match s {
        "run" | "trail_run" | "treadmill" | "run_virtual" => "run",
        "strength" | "workout" | "crossfit" => "strength",
        "ride" | "ride_virtual" | "ebike" => "ride",
        "walk" | "hike" => "walk",
        "yoga" | "pilates" | "stretch" => "mobility",
        _ => "",
    }
}

fn day_load(v: &[Item], rt: &BTreeMap<&str, f64>) -> f64 {
    v.iter().map(|i| i.min * rt.get(i.kind.as_str()).copied().unwrap_or(0.5)).sum()
}

// ---------- materialized days ----------

fn row(conn: &Connection, d: &str) -> Option<(Vec<Item>, String, String)> {
    conn.query_row("SELECT items, source, note FROM plan_days WHERE date = ?1", params![d], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default()))
    })
    .ok()
    .map(|(i, s, n)| (items_of(&serde_json::from_str(&i).unwrap_or(Value::Null)), s, n))
}

fn put(conn: &Connection, d: &str, items: &[Item], source: &str, note: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO plan_days (date, items, source, note, ts) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(date) DO UPDATE SET items = excluded.items, source = excluded.source, note = excluded.note, ts = excluded.ts",
        params![d, items_json(items).to_string(), source, note, store::now()],
    )
    .map_err(err)?;
    Ok(())
}

/// The template week for the Monday `m`, with the phase applied and checked against the headroom.
fn week_from_template(tpl: &[Vec<Item>], start: &str, m: &str, mode: &str, rt: &BTreeMap<&str, f64>, cap_week: Option<f64>, ret: Option<&(String, f64, usize)>) -> (Vec<Vec<Item>>, Vec<String>) {
    let (_, f, deload) = phase(start, m, mode);
    let wk = ((day_num(m).unwrap_or(0) - day_num(start).unwrap_or(0)) / 7).max(0) % 4 + 1;
    let mut notes = Vec::new();
    let mut w: Vec<Vec<Item>> = tpl
        .iter()
        .map(|d| {
            d.iter()
                .map(|i| {
                    let mut i = i.clone();
                    if deload && (i.kind == "tempo" || i.kind == "intervals") {
                        i.kind = "easy".into();
                    }
                    if i.kind != "ofp" && i.kind != "mobility" {
                        i.min = r5(i.min * f).max(20.0);
                    }
                    i
                })
                .collect()
        })
        .collect();
    if deload {
        notes.push("Разгрузочная неделя: объём меньше, без интенсивных тренировок — организм закрепляет прошлые три недели.".to_string());
    }
    // A control test once per 4-week block (week 3) instead of the first quality session.
    if wk == 3 && mode != "return" && ret.is_none() {
        if let Some(i) = w.iter_mut().flatten().find(|i| i.kind == "tempo" || i.kind == "intervals") {
            i.kind = "test".into();
            i.min = i.min.max(45.0);
            notes.push("На этой неделе контрольный тест 5 км — чтобы видеть прогресс к цели.".to_string());
        }
    }
    // Return to run: every run becomes the current step, not two days in a row.
    if let Some((k, min, st)) = ret {
        let mut prev = false;
        for d in w.iter_mut() {
            let had = d.iter().any(|i| is_run(&i.kind));
            d.retain(|i| !is_run(&i.kind));
            if had {
                if prev {
                    d.push(Item { kind: "bike".into(), min: 30.0 });
                    prev = false;
                } else {
                    d.insert(0, Item { kind: k.clone(), min: *min });
                    prev = true;
                }
            } else {
                prev = false;
            }
        }
        notes.push(format!("Возврат к бегу: шаг {} из {} — {}.", st + 1, crate::body_svc::STEPS.len(), crate::body_svc::STEPS[*st].2));
    }
    // Week headroom: never plan more than cap × your usual week.
    if let Some(cap) = cap_week.filter(|c| *c > 50.0) {
        let total: f64 = w.iter().map(|d| day_load(d, rt)).sum();
        if total > cap {
            let fixed: f64 = w.iter().flatten().filter(|i| !matches!(i.kind.as_str(), "easy" | "long" | "bike")).map(|i| i.min * rt.get(i.kind.as_str()).copied().unwrap_or(0.5)).sum();
            let flex = (total - fixed).max(1.0);
            let k = ((cap - fixed) / flex).clamp(0.5, 1.0);
            for d in w.iter_mut() {
                for i in d.iter_mut().filter(|i| matches!(i.kind.as_str(), "easy" | "long" | "bike")) {
                    i.min = r5(i.min * k).max(20.0);
                }
            }
            notes.push(format!("Объём урезан на {}%, чтобы неделя не превысила безопасный запас нагрузки.", ((1.0 - k) * 100.0).round()));
        }
    }
    (w, notes)
}

/// Makes sure the days from `from` for `n` days exist; template rows are refreshed, the ones you
/// changed or accepted stay.
fn materialize(conn: &Connection, from: &str, n: i64, rt: &BTreeMap<&str, f64>, cap_week: Option<f64>, force: bool) -> Result<(), String> {
    let (tpl, _, start) = template(conn);
    let md = mode(conn);
    let ret = crate::body_svc::return_item(conn);
    let mut weeks: BTreeMap<String, (Vec<Vec<Item>>, Vec<String>)> = BTreeMap::new();
    for k in 0..n {
        let d = add_days(from, k);
        let m = monday(&d);
        let (w, notes) = weeks.entry(m.clone()).or_insert_with(|| week_from_template(&tpl, &start, &m, &md, rt, cap_week, ret.as_ref())).clone();
        let existing = row(conn, &d);
        let keep = existing.as_ref().map(|(_, s, _)| s != "template").unwrap_or(false);
        if keep || (existing.is_some() && !force) {
            continue;
        }
        put(conn, &d, &w[wday(&d)], "template", &notes.join(" "))?;
    }
    // No two hard days in a row (a hard day after another one swaps with the next easy template day).
    for k in 0..n {
        let d = add_days(from, k);
        let prev = row(conn, &add_days(&d, -1));
        let Some((items, src, note)) = row(conn, &d) else { continue };
        if src != "template" || !hard(&items) || !prev.map(|p| hard(&p.0)).unwrap_or(false) {
            continue;
        }
        for j in 1..=3 {
            let e = add_days(&d, j);
            let Some((ei, es, en)) = row(conn, &e) else { break };
            let next_hard = row(conn, &add_days(&e, 1)).map(|x| hard(&x.0)).unwrap_or(false);
            if es == "template" && !hard(&ei) && !next_hard {
                put(conn, &e, &items, "template", &note)?;
                put(conn, &d, &ei, "template", &en)?;
                break;
            }
        }
    }
    Ok(())
}

// ---------- done marks ----------

fn acts_by_day(acts: &[engine_svc::Cached], tz: i64) -> BTreeMap<String, Vec<&engine_svc::Cached>> {
    let mut m: BTreeMap<String, Vec<&engine_svc::Cached>> = BTreeMap::new();
    for a in acts {
        m.entry(day_of(a.start_utc, tz)).or_default().push(a);
    }
    m
}

fn status(items: &[Item], done: &[&engine_svc::Cached], d: &str, today: &str) -> &'static str {
    let need: Vec<&Item> = items.iter().filter(|i| i.kind != "rest").collect();
    if need.is_empty() {
        return "rest";
    }
    let got = need.iter().filter(|i| {
        let g = kind(&i.kind).2;
        done.iter().any(|a| {
            let sg = sport_group(&a.sport);
            sg == g || (g == "mobility" && sg == "strength") || (i.kind == "walkrun" && sg == "walk")
        })
    }).count();
    if got == need.len() {
        "done"
    } else if got > 0 {
        "partial"
    } else if d < today {
        "missed"
    } else {
        "planned"
    }
}

// ---------- workout of the day ----------

fn pace(s: f64) -> String {
    let s = s.round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Warm-up, main part, cool-down and drills for one item.
pub fn detail(i: &Item, p: &crate::engine::Profile, injuries: &[String]) -> Value {
    let z = &p.zones;
    let pz = &p.pace_zones;
    let hr = |a: usize, b: usize| format!("пульс {:.0}–{:.0}", z[a], z[b]);
    let pc = |a: usize, b: usize| format!("темп {}–{} /км", pace(pz[b]), pace(pz[a]));
    let m = i.min.round();
    let knees = injuries.iter().any(|x| x.contains("Колен"));
    let achil = injuries.iter().any(|x| x.contains("Ахилл") || x.contains("Голень"));
    let back = injuries.iter().any(|x| x.contains("Спина"));
    let drills = "Беговые упражнения по 2×20 м: семенящий бег, высокое бедро, захлёст голени, прыжковый бег; 3–4 ускорения по 80 м на 90% с возвратом шагом.";
    let (warm, main, cool, dr, notes): (String, String, String, Option<&str>, Vec<String>) = match i.kind.as_str() {
        "easy" => ("5–10 минут шагом и лёгкой трусцой, суставная разминка.".into(),
            format!("{} мин ровно в Z2: {}, {}. Можно говорить полными фразами.", (m - 10.0).max(15.0), hr(1, 2), pc(1, 2)),
            "5 минут шагом, растяжка икр, задней поверхности бедра и квадрицепсов.".into(), None,
            vec!["Если чувствуешь себя хорошо — в конце 4 ускорения по 20 секунд.".into()]),
        "long" => ("10 минут: шаг, затем трусца в Z1.".into(),
            format!("{} мин в Z2: {}, {}. Последние 15 минут можно держать верх Z2.", (m - 15.0).max(30.0), hr(1, 2), pc(1, 2)),
            "5 минут шагом, растяжка, роллер для икр и бёдер.".into(), None,
            { let mut n = vec!["Вода по ходу: 150–200 мл каждые 20 минут, если бег дольше часа.".to_string()];
              if m >= 75.0 { n.push("Дольше 75 минут — углеводы: гель или 30–60 г углеводов в час.".into()); } n }),
        "tempo" => {
            let reps = ((m - 30.0) / 10.0).floor().clamp(2.0, 4.0);
            ("15 минут в Z1–Z2 и беговые упражнения.".into(),
             format!("{} × 8 мин в Z3–Z4 (около порога): {}, {}; между отрезками 2 мин трусцы.", reps, hr(2, 4), pc(3, 4)),
             "10 минут трусцы в Z1 и растяжка.".into(), Some(drills),
             vec!["Темп должен быть «комфортно тяжёлым»: говорить можно только короткими фразами.".into()])
        }
        "intervals" => {
            let reps = ((m - 30.0) / 5.0).floor().clamp(4.0, 6.0);
            ("15 минут в Z1–Z2, беговые упражнения и ускорения.".into(),
             format!("{} × 3 мин в Z5: пульс от {:.0}, темп быстрее {} /км; отдых 2 мин трусцы.", reps, z[4], pace(pz[4])),
             "10 минут трусцы в Z1, растяжка.".into(), Some(drills),
             vec!["Если на 3-м отрезке темп падает больше чем на 10 секунд — заканчивай, это сигнал усталости.".into()])
        }
        "gym" => {
            let squat = if knees { "ягодичный мост со штангой 3×10 (вместо глубокого приседа — бережём колени)" } else { "присед со штангой 3×8" };
            let hinge = if back { "ягодичный мост на одной ноге 3×10 (вместо тяги — бережём спину)" } else { "румынская тяга 3×8" };
            let lunge = if knees { "зашагивание на невысокую тумбу 3×10" } else { "выпады назад 3×10 на ногу" };
            ("10 минут: вело или гребля, суставная разминка, 2 разминочных подхода с лёгким весом.".into(),
             format!("{squat}; {hinge}; {lunge}; подъёмы на носки 3×15; жим гантелей 3×10; тяга в наклоне 3×10; планка 3×40 с. Вес — чтобы 2 повтора оставались в запасе."),
             "5–10 минут растяжки.".into(), None,
             vec!["Если на следующий день интервалы или длительный — ноги лучше нагружать умеренно.".into()])
        }
        "ofp" => ("3–5 минут суставной разминки.".into(),
            format!("{} мин по кругу, 2–3 круга: ягодичный мост 15, боковая планка 30 с на сторону, «мёртвый жук» 10, клэмшелл с резинкой 15, {}, обратные выпады 10.", m,
                if achil { "медленные опускания на носке с края ступеньки 3×15 (для ахилла и голени)" } else { "подъёмы на носки 20" }),
            "Растяжка 5 минут.".into(), None,
            vec!["Это укрепление «от травм»: мышцы таза, кора и стопы держат колени и ахилл.".into()]),
        "upper" => ("5 минут суставной разминки плеч и грудного отдела.".into(),
            format!("{} мин, 3 круга: отжимания 10–15, тяга гантели в наклоне 12, жим гантелей стоя 10, подтягивания или тяга резины 10, планка 40 с, боковая планка 30 с. Ноги не нагружаем.", m),
            "Растяжка груди и плеч.".into(), None, vec!["Сохраняет силу и форму, пока ноги восстанавливаются.".into()]),
        "walkrun" => ("5 минут бодрым шагом.".into(),
            format!("{} мин: чередование бега и ходьбы по шагу возврата. Бег очень лёгкий, в Z1–Z2 ({}). Боль во время — не выше 3 из 10, иначе переходи на шаг.", m, hr(0, 2)),
            "5 минут шагом, мягкая растяжка.".into(), None, vec!["Завтра утром отметь, как себя чувствует это место — от этого зависит следующий шаг.".into()]),
        "test" => ("15 минут в Z1–Z2, беговые упражнения, 3 ускорения по 20 секунд.".into(),
            format!("5 км максимально ровно, на пределе, но без «взрыва» в начале: первый километр чуть медленнее целевого темпа. Ориентир по прошлым тренировкам — {} /км и быстрее.", pace(pz[4])),
            "10–15 минут трусцы в Z1 и растяжка.".into(), Some(drills),
            vec!["Лучше на ровном месте или стадионе, в похожую погоду — чтобы сравнивать с прошлым тестом.".into(), "Если готовность сегодня низкая — тест лучше перенести, Kouros предложит.".into()]),
        "bike" => ("5–10 минут спокойно.".into(), format!("{} мин в Z2: {}.", (m - 10.0).max(15.0), hr(1, 2)), "5 минут спокойно, растяжка.".into(), None, vec![]),
        "walk" => ("".into(), format!("{} мин в спокойном темпе, можно с небольшими подъёмами.", m), "".into(), None, vec!["Прогулка ускоряет восстановление и почти не нагружает связки.".into()]),
        "mobility" => ("".into(), format!("{} мин: растяжка икр, задней поверхности бедра, сгибателей бедра, грудного отдела; роллер.", m), "".into(), None, vec![]),
        _ => ("".into(), "День отдыха. Прогулка 30–40 минут и 10 минут мобилити — по желанию.".into(), "".into(), None, vec![]),
    };
    json!({"kind": i.kind, "title": kind(&i.kind).1, "min": m, "warmup": warm, "main": main, "cooldown": cool, "drills": dr, "notes": notes})
}

// ---------- proposals ----------

/// What fits the allowed tier instead of the planned items.
fn fit(items: &[Item], allowed: usize, t: f64) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for i in items {
        let k = kind(&i.kind);
        if k.4 <= allowed {
            out.push(i.clone());
            continue;
        }
        let n = match (k.0, allowed) {
            ("intervals", 5) => Item { kind: "tempo".into(), min: i.min },
            (_, 4) if k.2 == "run" => Item { kind: "easy".into(), min: r5(t.max(40.0)) },
            (_, 3) if k.2 == "run" => Item { kind: "easy".into(), min: r5((i.min * 0.7).clamp(30.0, 50.0)) },
            ("gym", 3) => Item { kind: "ofp".into(), min: 30.0 },
            (_, 2) if k.2 == "run" => Item { kind: "walk".into(), min: 45.0 },
            (_, 2) => Item { kind: "mobility".into(), min: 20.0 },
            (_, 1) => Item { kind: "walk".into(), min: 30.0 },
            _ => Item { kind: "rest".into(), min: 0.0 },
        };
        if !out.contains(&n) {
            out.push(n);
        }
    }
    if allowed <= 2 && !out.iter().any(|i| i.kind == "mobility") && out.iter().any(|i| i.kind == "walk") {
        out.push(Item { kind: "mobility".into(), min: 15.0 });
    }
    out
}

fn change_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let b: String = r.get(3)?;
    let a: String = r.get(4)?;
    let bi = items_of(&serde_json::from_str(&b).unwrap_or(Value::Null));
    let ai = items_of(&serde_json::from_str(&a).unwrap_or(Value::Null));
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "ts": r.get::<_, i64>(1)?, "date": r.get::<_, String>(2)?, "dateRu": date_ru(&r.get::<_, String>(2)?),
        "before": items_json(&bi), "after": items_json(&ai), "beforeText": items_text(&bi), "afterText": items_text(&ai),
        "reason": r.get::<_, String>(5)?, "origin": r.get::<_, String>(6)?, "state": r.get::<_, String>(7)?, "group": r.get::<_, Option<i64>>(8)?,
    }))
}

fn add_change(conn: &Connection, d: &str, before: &[Item], after: &[Item], reason: &str, origin: &str, state: &str, group: Option<i64>) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO plan_changes (ts, date, before, after, reason, origin, state, grp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![store::now(), d, items_json(before).to_string(), items_json(after).to_string(), reason, origin, state, group],
    )
    .map_err(err)?;
    Ok(conn.last_insert_rowid())
}

fn auto_apply(conn: &Connection) -> bool {
    coach_svc::profile(conn)["planChanges"].as_str() == Some("auto")
}

/// Today's check: if the plan asks for more than the body and the gates allow, propose a swap
/// (and move a key workout to a later easy day).
fn check_today(conn: &Connection, s: &Value, today: &str, t: f64) -> Result<(), String> {
    let Some((items, src, _)) = row(conn, today) else { return Ok(()) };
    if src == "chosen" {
        return Ok(()); // 0.23: the athlete chose today's workout — the choice was checked then
    }
    let tier = s["day"]["advice"]["tier"].as_str().and_then(|x| crate::engine::daily::TIERS.iter().position(|t| *t == x));
    let Some(allowed) = tier else { return Ok(()) };
    if s["day"]["todayLoad"].as_f64().unwrap_or(0.0) > 0.0 && s["day"]["todayLoad"].as_f64().unwrap_or(0.0) >= s["day"]["target"]["lo"].as_f64().unwrap_or(f64::MAX) {
        return Ok(());
    }
    if need_tier(&items) <= allowed {
        return Ok(());
    }
    // One Kouros proposal a day per tier (a later, lower tier may propose again).
    let tag = format!("tier:{allowed}");
    let seen: i64 = conn
        .query_row("SELECT count(*) FROM plan_changes WHERE date = ?1 AND origin = 'kouros' AND reason LIKE ?2", params![today, format!("%[{tag}]%")], |r| r.get(0))
        .unwrap_or(0);
    if seen > 0 {
        return Ok(());
    }
    let after = fit(&items, allowed, t);
    let score = s["readiness"]["score"].as_f64();
    let reasons: Vec<String> = s["day"]["advice"]["reasons"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let why = if !reasons.is_empty() {
        reasons.join("; ")
    } else {
        format!("готовность {} — сегодня тело готово только на «{}»", score.map(|x| format!("{x:.0}")).unwrap_or("—".into()), s["day"]["advice"]["title"].as_str().unwrap_or(""))
    };
    let state = if auto_apply(conn) { "accepted" } else { "proposed" };
    let gid = add_change(conn, today, &items, &after, &format!("{why} [{tag}]"), "kouros", state, None)?;
    conn.execute("UPDATE plan_changes SET grp = ?1 WHERE id = ?1", params![gid]).map_err(err)?;
    if state == "accepted" {
        put(conn, today, &after, "accepted", "")?;
    }
    // A key workout dropped today goes to the next easy day in 1–3 days.
    let key: Vec<Item> = items.iter().filter(|i| kind(&i.kind).3).cloned().collect();
    if !key.is_empty() {
        for j in 1..=3 {
            let d = add_days(today, j);
            let Some((di, ds, _)) = row(conn, &d) else { break };
            let nb = row(conn, &add_days(&d, 1)).map(|x| hard(&x.0)).unwrap_or(false);
            if ds == "template" && !hard(&di) && !nb && di.iter().all(|i| i.kind != "gym") {
                let mut moved = key.clone();
                moved.extend(di.iter().filter(|i| i.kind == "ofp").cloned());
                add_change(conn, &d, &di, &moved, &format!("перенос с {} [{tag}]", date_ru(today)), "kouros", state, Some(gid))?;
                if state == "accepted" {
                    put(conn, &d, &moved, "accepted", "")?;
                }
                break;
            }
        }
    }
    Ok(())
}

/// Accept / keep a proposal (and the ones grouped with it).
pub fn decide(conn: &Connection, id: i64, accept: bool) -> Result<(), String> {
    let grp: Option<i64> = conn.query_row("SELECT grp FROM plan_changes WHERE id = ?1", params![id], |r| r.get(0)).map_err(err)?;
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM plan_changes WHERE (id = ?1 OR (grp IS NOT NULL AND grp = ?2)) AND state = 'proposed'").map_err(err)?;
        let v = st.query_map(params![id, grp.unwrap_or(-1)], |r| r.get(0)).map_err(err)?.flatten().collect();
        v
    };
    for i in ids {
        let (d, after): (String, String) = conn.query_row("SELECT date, after FROM plan_changes WHERE id = ?1", params![i], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?;
        if accept {
            put(conn, &d, &items_of(&serde_json::from_str(&after).unwrap_or(Value::Null)), "accepted", "")?;
        }
        conn.execute("UPDATE plan_changes SET state = ?2, ts = ?3 WHERE id = ?1", params![i, if accept { "accepted" } else { "kept" }, store::now()]).map_err(err)?;
    }
    Ok(())
}

/// A day changed by hand.
pub fn set_day(conn: &Connection, d: &str, items: &Value) -> Result<(), String> {
    let new = items_of(items);
    let before = row(conn, d).map(|x| x.0).unwrap_or_default();
    if before == new {
        return Ok(());
    }
    put(conn, d, &new, "manual", "")?;
    add_change(conn, d, &before, &new, "изменено вручную", "manual", "accepted", None)?;
    Ok(())
}

/// Back to the template for one day.
pub fn reset_day(conn: &Connection, d: &str) -> Result<(), String> {
    conn.execute("DELETE FROM plan_days WHERE date = ?1", params![d]).map_err(err)?;
    Ok(())
}

pub fn save_template_days(conn: &Connection, days: &Value, rebuild: bool) -> Result<(), String> {
    let today = engine_svc::today(&engine_svc::profile(conn));
    let (_, _, start) = template(conn);
    if rebuild {
        let t = build_template(&coach_svc::profile(conn));
        save_template(conn, &t, true, &start)?;
    } else {
        let v: Vec<Vec<Item>> = days.as_array().ok_or("Ожидалась неделя")?.iter().map(items_of).collect();
        if v.len() != 7 {
            return Err("Нужно 7 дней".into());
        }
        save_template(conn, &v, false, &start)?;
    }
    // Template rows from today on are rebuilt next time (today only if nothing is done yet).
    conn.execute("DELETE FROM plan_days WHERE date > ?1 AND source = 'template'", params![today]).map_err(err)?;
    Ok(())
}

/// After the onboarding is saved: a template still built automatically follows the new answers.
pub fn on_profile_saved(conn: &Connection) -> Result<(), String> {
    let (_, auto, start) = template(conn);
    if auto {
        let t = build_template(&coach_svc::profile(conn));
        save_template(conn, &t, true, &start)?;
        let today = engine_svc::today(&engine_svc::profile(conn));
        conn.execute("DELETE FROM plan_days WHERE date > ?1 AND source = 'template'", params![today]).map_err(err)?;
    }
    Ok(())
}

// ---------- Claude's [ПЛАН] lines ----------

fn parse_date(s: &str, today: &str) -> Option<String> {
    let low = s.to_lowercase();
    if let Some(p) = low.find("20") {
        if let Some(d) = low.get(p..p + 10) {
            if day_num(d).is_some() && d.chars().nth(4) == Some('-') {
                return Some(d.to_string());
            }
        }
    }
    let y: i64 = today.get(0..4)?.parse().ok()?;
    let nums: Vec<i64> = low.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
    let day = *nums.first()?;
    let month = MONTHS
        .iter()
        .position(|m| low.contains(m))
        .or_else(|| if low.contains("май") { Some(4) } else { None })
        .map(|m| m as i64 + 1)
        .or_else(|| nums.get(1).copied())
        .or_else(|| today.get(5..7)?.parse().ok())?;
    if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    let mut d = format!("{y:04}-{month:02}-{day:02}");
    if d.as_str() < add_days(today, -60).as_str() {
        d = format!("{:04}-{month:02}-{day:02}", y + 1);
    }
    day_num(&d).map(|_| d)
}

fn parse_items(s: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for part in s.split(|c| c == '+' || c == ',' || c == ';') {
        let p = part.to_lowercase();
        let min = p.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse::<f64>().ok()).find(|x| *x >= 5.0 && *x <= 300.0);
        let k = if p.contains("отдых") || p.contains("выходн") {
            "rest"
        } else if p.contains("тест") || p.contains("контрольн") {
            "test"
        } else if p.contains("верх тела") || p.contains("верх") && p.contains("кор") {
            "upper"
        } else if (p.contains("бег") && p.contains("ходьб")) || p.contains("чередов") {
            "walkrun"
        } else if p.contains("интервал") {
            "intervals"
        } else if p.contains("темп") || p.contains("порог") {
            "tempo"
        } else if p.contains("длительн") || p.contains("длинн") {
            "long"
        } else if p.contains("вело") {
            "bike"
        } else if p.contains("зал") || p.contains("силов") {
            "gym"
        } else if p.contains("офп") || p.contains("укрепл") || p.contains("кор") {
            "ofp"
        } else if p.contains("растяж") || p.contains("мобил") || p.contains("йог") {
            "mobility"
        } else if p.contains("ходьб") || p.contains("прогул") {
            "walk"
        } else if p.contains("бег") || p.contains("пробеж") {
            "easy"
        } else {
            continue;
        };
        let def = match k { "rest" => 0.0, "long" => 75.0, "ofp" | "mobility" => 25.0, "walk" => 45.0, _ => 45.0 };
        out.push(Item { kind: k.into(), min: min.unwrap_or(def) });
    }
    out
}

/// Turns «[ПЛАН] ср 22 окт: интервалы → лёгкий бег 40 мин Z2» lines into proposals. Returns how many.
pub fn from_claude(conn: &Connection, text: &str) -> usize {
    let today = engine_svc::today(&engine_svc::profile(conn));
    let mut n = 0;
    let mut grp: Option<i64> = None;
    for line in text.lines() {
        let l = line.trim().trim_start_matches(['-', '*', '•', ' ']);
        let Some(rest) = l.strip_prefix("[ПЛАН]").or_else(|| l.strip_prefix("[План]")).or_else(|| l.strip_prefix("[план]")) else { continue };
        let (dpart, what) = match rest.split_once(':') {
            Some(x) => x,
            None => continue,
        };
        let Some(d) = parse_date(dpart, &today) else { continue };
        if d.as_str() < today.as_str() {
            continue;
        }
        let target = what.rsplit_once('→').map(|x| x.1).or_else(|| what.rsplit_once("->").map(|x| x.1)).unwrap_or(what);
        let after = parse_items(target);
        if after.is_empty() {
            continue;
        }
        let before = row(conn, &d).map(|x| x.0).unwrap_or_default();
        if before == after {
            continue;
        }
        if let Ok(id) = add_change(conn, &d, &before, &after, &format!("совет Claude: {}", what.trim()), "claude", "proposed", grp) {
            if grp.is_none() {
                grp = Some(id);
                let _ = conn.execute("UPDATE plan_changes SET grp = ?1 WHERE id = ?1", params![id]);
            }
            n += 1;
        }
    }
    n
}

/// Plan text and hard flag of one day (for the recovery tools).
pub fn day_items_text(conn: &Connection, d: &str) -> String {
    row(conn, d).map(|x| items_text(&x.0)).unwrap_or_default()
}

pub fn day_hard(conn: &Connection, d: &str) -> bool {
    row(conn, d).map(|x| hard(&x.0)).unwrap_or(false)
}

/// Template days from today on are rebuilt next time (after a return-to-run step changes).
pub fn refresh_future(conn: &Connection) -> Result<(), String> {
    let today = engine_svc::today(&engine_svc::profile(conn));
    conn.execute("DELETE FROM plan_days WHERE date >= ?1 AND source = 'template'", params![today]).map_err(err)?;
    Ok(())
}

/// Active pain vs the next 3 days: running or strength that loads the painful place gets an
/// alternative that keeps the shape (never "train through pain").
fn pain_check(conn: &Connection, today: &str, t: f64) -> Result<(), String> {
    let pains = crate::body_svc::active(conn);
    if pains.is_empty() {
        return Ok(());
    }
    let ret = crate::body_svc::return_item(conn).is_some();
    for k in 0..3 {
        let d = add_days(today, k);
        let Some((items, src, _)) = row(conn, &d) else { continue };
        if src == "chosen" {
            continue; // the athlete's own choice was already checked against the pain
        }
        for (area, name, level, runs, upper) in &pains {
            if *level < 4 {
                continue;
            }
            let tag = format!("[pain:{area}]");
            let seen: i64 = conn
                .query_row("SELECT count(*) FROM plan_changes WHERE date = ?1 AND reason LIKE ?2", params![d, format!("%{tag}%")], |r| r.get(0))
                .unwrap_or(0);
            if seen > 0 {
                continue;
            }
            let hit = items.iter().any(|i| (*runs && is_run(&i.kind) && !(ret && i.kind == "walkrun")) || (*upper && matches!(i.kind.as_str(), "gym" | "upper")));
            if !hit {
                continue;
            }
            let mut after: Vec<Item> = Vec::new();
            for i in &items {
                if *runs && is_run(&i.kind) {
                    let n = if area.as_str() == "knees" && *level >= 6 {
                        Item { kind: "upper".into(), min: 40.0 }
                    } else {
                        Item { kind: "bike".into(), min: r5((i.min * 0.8).clamp(30.0, t.max(45.0))) }
                    };
                    if !after.contains(&n) {
                        after.push(n);
                    }
                } else if *upper && matches!(i.kind.as_str(), "gym" | "upper") {
                    let n = if area.as_str() == "back" { Item { kind: "mobility".into(), min: 20.0 } } else { Item { kind: "ofp".into(), min: 30.0 } };
                    if !after.contains(&n) {
                        after.push(n);
                    }
                } else {
                    after.push(i.clone());
                }
            }
            let why = format!("боль: {} {}/10 — через боль не тренируемся, замена сохранит форму {tag}", name.to_lowercase(), level);
            let state = if auto_apply(conn) { "accepted" } else { "proposed" };
            add_change(conn, &d, &items, &after, &why, "kouros", state, None)?;
            if state == "accepted" {
                put(conn, &d, &after, "accepted", "")?;
            }
        }
    }
    Ok(())
}

/// Fluoroquinolone course (0.18): until 14 days after the last intake no tempo, intervals, tests or
/// long runs — they become easy runs (tendon risk, especially the Achilles).
fn med_check(conn: &Connection, today: &str) -> Result<(), String> {
    let Some((until, name)) = crate::meds_svc::fq_until(conn, today) else { return Ok(()) };
    for k in 0..7 {
        let d = add_days(today, k);
        if d > until {
            break;
        }
        let Some((items, _, _)) = row(conn, &d) else { continue };
        if !items.iter().any(|i| matches!(i.kind.as_str(), "tempo" | "intervals" | "test" | "long")) {
            continue;
        }
        let tag = "[med:fq]";
        let seen: i64 = conn
            .query_row("SELECT count(*) FROM plan_changes WHERE date = ?1 AND reason LIKE ?2", params![d, format!("%{tag}%")], |r| r.get(0))
            .unwrap_or(0);
        if seen > 0 {
            continue;
        }
        let after: Vec<Item> = items
            .iter()
            .map(|i| match i.kind.as_str() {
                "tempo" | "intervals" | "test" => Item { kind: "easy".into(), min: r5(i.min * 0.8).max(30.0) },
                "long" => Item { kind: "easy".into(), min: r5(i.min * 0.7).clamp(30.0, 60.0) },
                _ => i.clone(),
            })
            .collect();
        let why = format!("{name}: курс фторхинолона — до {} без интенсивных и длительных, бережём сухожилия {tag}", date_ru(&until));
        let state = if auto_apply(conn) { "accepted" } else { "proposed" };
        add_change(conn, &d, &items, &after, &why, "kouros", state, None)?;
        if state == "accepted" {
            put(conn, &d, &after, "accepted", "")?;
        }
    }
    Ok(())
}

/// C-reactive protein above 5 mg/l in a sample of the last 7 days (0.20): the 3 days after the
/// sample have no tempo, intervals or tests (they become easy runs) and long runs are shortened.
fn lab_check(conn: &Connection, today: &str) -> Result<(), String> {
    let Some((date, v)) = crate::labs_svc::crp_recent(conn, today) else { return Ok(()) };
    let until = add_days(&date, 3);
    for k in 0..4 {
        let d = add_days(today, k);
        if d > until {
            break;
        }
        let Some((items, _, _)) = row(conn, &d) else { continue };
        if !items.iter().any(|i| matches!(i.kind.as_str(), "tempo" | "intervals" | "test" | "long")) {
            continue;
        }
        let tag = format!("[lab:crp:{date}]");
        let seen: i64 = conn
            .query_row("SELECT count(*) FROM plan_changes WHERE date = ?1 AND reason LIKE ?2", params![d, format!("%{tag}%")], |r| r.get(0))
            .unwrap_or(0);
        if seen > 0 {
            continue;
        }
        let after: Vec<Item> = items
            .iter()
            .map(|i| match i.kind.as_str() {
                "tempo" | "intervals" | "test" => Item { kind: "easy".into(), min: r5(i.min * 0.8).max(30.0) },
                "long" => Item { kind: "easy".into(), min: r5(i.min * 0.7).clamp(30.0, 60.0) },
                _ => i.clone(),
            })
            .collect();
        let why = format!("С-реактивный белок {} мг/л в анализе от {} — признак воспаления, пока без интенсивных {tag}", format!("{v:.1}").replace('.', ","), date_ru(&date));
        let state = if auto_apply(conn) { "accepted" } else { "proposed" };
        add_change(conn, &d, &items, &after, &why, "kouros", state, None)?;
        if state == "accepted" {
            put(conn, &d, &after, "accepted", "")?;
        }
    }
    Ok(())
}

/// Does a done workout correspond to something planned that day?
fn planned_match(items: &[Item], a: &engine_svc::Cached) -> bool {
    let sg = sport_group(&a.sport);
    items.iter().any(|i| {
        let g = kind(&i.kind).2;
        !g.is_empty() && (g == sg || (g == "mobility" && sg == "strength") || (i.kind == "walkrun" && sg == "walk"))
    })
}

/// Workouts done outside the plan (yesterday and today): light → swap with tomorrow's easy run
/// (tomorrow rest), hard → no hard day right after it, a quality-like run → drop the next quality
/// session; and the rest of the week is trimmed if it would pass the headroom.
fn unplanned_check(conn: &Connection, today: &str, by: &BTreeMap<String, Vec<&engine_svc::Cached>>, rt: &BTreeMap<&str, f64>, cap_week: Option<f64>, usual_day: f64) -> Result<(), String> {
    let state = if auto_apply(conn) { "accepted" } else { "proposed" };
    let seen = |tag: &str| -> bool {
        conn.query_row("SELECT count(*) FROM plan_changes WHERE reason LIKE ?1", params![format!("%{tag}%")], |r| r.get::<_, i64>(0)).map(|n| n > 0).unwrap_or(true)
    };
    let apply = |d: &str, before: &[Item], after: &[Item], why: &str, grp: Option<i64>| -> Result<i64, String> {
        let id = add_change(conn, d, before, after, why, "kouros", state, grp)?;
        if state == "accepted" {
            put(conn, d, after, "accepted", "")?;
        }
        Ok(id)
    };
    for d in [add_days(today, -1), today.to_string()] {
        let items = row(conn, &d).map(|x| x.0).unwrap_or_default();
        for a in by.get(&d).cloned().unwrap_or_default() {
            if planned_match(&items, a) || a.duration_s < 900.0 || sport_group(&a.sport) == "walk" {
                continue;
            }
            let tag = format!("[unplanned:{}]", a.id);
            if seen(&tag) {
                continue;
            }
            let run = sport_group(&a.sport) == "run";
            let heavy = a.load > usual_day.max(1.0) * 1.3 || (run && a.load >= rt.get("tempo").copied().unwrap_or(1.6) * 40.0);
            let next = add_days(&d, 1);
            if next.as_str() < today {
                continue;
            }
            let Some((ni, nsrc, _)) = row(conn, &next) else { continue };
            let when = if d.as_str() == today { "сегодня" } else { "вчера" };
            if heavy && hard(&ni) {
                let after: Vec<Item> = ni.iter().map(|i| if kind(&i.kind).3 { Item { kind: "easy".into(), min: 30.0 } } else { i.clone() }).collect();
                let gid = apply(&next, &ni, &after, &format!("{when} была тяжёлая тренировка вне плана — два тяжёлых дня подряд не ставим {tag}"), None)?;
                conn.execute("UPDATE plan_changes SET grp = ?1 WHERE id = ?1", params![gid]).map_err(err)?;
                let key: Vec<Item> = ni.iter().filter(|i| kind(&i.kind).3).cloned().collect();
                for j in 2..=4 {
                    let e = add_days(&d, j);
                    let Some((ei, es, _)) = row(conn, &e) else { break };
                    let nb = row(conn, &add_days(&e, 1)).map(|x| hard(&x.0)).unwrap_or(false);
                    if es == "template" && !hard(&ei) && !nb {
                        apply(&e, &ei, &key, &format!("перенос с {} {tag}", date_ru(&next)), Some(gid))?;
                        break;
                    }
                }
            } else if heavy && run {
                // A quality-like run was done: the next quality session within 3 days becomes easy.
                for j in 1..=3 {
                    let e = add_days(&d, j);
                    if e.as_str() < today {
                        continue;
                    }
                    let Some((ei, _, _)) = row(conn, &e) else { break };
                    if ei.iter().any(|i| i.kind == "tempo" || i.kind == "intervals") {
                        let after: Vec<Item> = ei.iter().map(|i| if i.kind == "tempo" || i.kind == "intervals" { Item { kind: "easy".into(), min: r5(i.min * 0.8).max(30.0) } } else { i.clone() }).collect();
                        apply(&e, &ei, &after, &format!("{when} уже была интенсивная пробежка вне плана — эта неделя своё качество получила {tag}"), None)?;
                        break;
                    }
                }
            } else if run && nsrc == "template" && ni.iter().any(|i| i.kind == "easy") && !hard(&ni) {
                let after: Vec<Item> = ni.iter().filter(|i| i.kind != "easy").cloned().collect();
                let after = if after.is_empty() { vec![Item { kind: "rest".into(), min: 0.0 }] } else { after };
                apply(&next, &ni, &after, &format!("{when} уже был лёгкий бег вне плана — поменяли местами: завтра отдых, объём недели тот же {tag}"), None)?;
            }
        }
    }
    // The rest of the week vs the headroom.
    if let Some(cap) = cap_week.filter(|c| *c > 50.0) {
        let m = monday(today);
        let tag = format!("[week:{m}]");
        if !seen(&tag) {
            let done: f64 = (0..7).map(|k| add_days(&m, k)).filter(|d| d.as_str() <= today).filter_map(|d| by.get(&d)).flatten().map(|a| a.load).sum();
            let rest: Vec<(String, Vec<Item>)> = (0..7).map(|k| add_days(&m, k)).filter(|d| d.as_str() > today).filter_map(|d| row(conn, &d).map(|x| (d, x.0))).collect();
            let planned: f64 = rest.iter().map(|(_, i)| day_load(i, rt)).sum();
            if done + planned > cap * 1.05 && planned > 0.0 {
                let flex: f64 = rest.iter().flat_map(|(_, i)| i.iter()).filter(|i| matches!(i.kind.as_str(), "easy" | "long" | "bike")).map(|i| i.min * rt.get(i.kind.as_str()).copied().unwrap_or(0.5)).sum();
                if flex > 0.0 {
                    let k = (1.0 - (done + planned - cap) / flex).clamp(0.3, 0.95);
                    let mut grp: Option<i64> = None;
                    for (d, items) in &rest {
                        if !items.iter().any(|i| matches!(i.kind.as_str(), "easy" | "long" | "bike")) {
                            continue;
                        }
                        let after: Vec<Item> = items.iter().map(|i| if matches!(i.kind.as_str(), "easy" | "long" | "bike") { Item { kind: i.kind.clone(), min: r5(i.min * k).max(20.0) } } else { i.clone() }).collect();
                        let id = apply(d, items, &after, &format!("неделя уже {} из безопасных {} — остальные дни короче {tag}", done.round(), cap.round()), grp)?;
                        if grp.is_none() {
                            grp = Some(id);
                            conn.execute("UPDATE plan_changes SET grp = ?1 WHERE id = ?1", params![id]).map_err(err)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------- state ----------

pub fn state(conn: &mut Connection) -> Result<Value, String> {
    let s = engine_svc::summary(conn)?;
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let tz = p.tz_offset_s;
    let rest = engine_svc::resting_hr(conn, &p);
    let acts = engine_svc::refresh_metrics(conn, &p, rest)?;
    let rt = rates(&acts, &today, tz);
    let ctl = s["load"]["ctl"].as_f64().unwrap_or(0.0);
    let cap_week = if ctl > 5.0 { Some(ctl * 7.0 * p.week_cap) } else { None };
    let cp = coach_svc::profile(conn);
    let t = cp["time"].as_str().and_then(|x| x.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(60.0);
    let injuries: Vec<String> = cp["injuries"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let has_tpl = get_json(conn, "plan_template")["days"].is_array();
    if !has_tpl {
        let (tpl, auto, start) = template(conn);
        save_template(conn, &tpl, auto, &start)?;
    }
    let from = add_days(&today, -6);
    materialize(conn, &today, 21, &rt, cap_week, false)?;
    // 1.3.2 (owner): the plan is checked once the night is in — today's readiness frozen after sleep
    // (or 15:00 without a recorded night) — not at midnight, when the day has no readiness yet.
    let morning_in = conn.query_row("SELECT 1 FROM readiness_log WHERE date = ?1", params![today], |_| Ok(())).is_ok()
        || (crate::store::now() + tz).rem_euclid(86400) >= 15 * 3600;
    if morning_in {
        check_today(conn, &s, &today, t)?;
        pain_check(conn, &today, t)?;
        med_check(conn, &today)?;
        lab_check(conn, &today)?;
        let by0 = acts_by_day(&acts, tz);
        unplanned_check(conn, &today, &by0, &rt, cap_week, s["day"]["usualLoad"].as_f64().unwrap_or(0.0))?;
    }
    let by = acts_by_day(&acts, tz);
    let (tpl, auto, start) = template(conn);
    let md = mode(conn);
    let mut days = Vec::new();
    for k in 0..28 {
        let d = add_days(&from, k);
        let r = row(conn, &d);
        let items = r.as_ref().map(|x| x.0.clone()).unwrap_or_default();
        let done = by.get(&d).cloned().unwrap_or_default();
        let (block, _, deload) = phase(&start, &d, &md);
        let wk = ((day_num(&monday(&d)).unwrap_or(0) - day_num(&start).unwrap_or(0)) / 7).max(0) % 4 + 1;
        let st = if r.is_some() || d.as_str() <= today.as_str() { status(&items, &done, &d, &today) } else { "planned" };
        let note = r.as_ref().map(|x| x.2.clone()).filter(|x| !x.is_empty());
        let det: Vec<Value> = items.iter().filter(|i| i.kind != "rest").map(|i| detail(i, &p, &injuries)).collect();
        let da: Vec<Value> = done.iter().map(|a| json!({"id": a.id, "sport": a.sport, "min": (a.duration_s / 60.0).round(), "load": a.load.round(), "unplanned": !planned_match(&items, a) && sport_group(&a.sport) != "walk"})).collect();
        days.push(json!({
            "date": d, "dateRu": date_ru(&d), "wd": WD[wday(&d)], "items": items_json(&items), "text": items_text(&items),
            "source": r.as_ref().map(|x| x.1.clone()), "note": note,
            "confirmed": conn.query_row("SELECT COALESCE(confirmed, 0) FROM plan_days WHERE date = ?1", params![d], |q| q.get::<_, i64>(0)).unwrap_or(0) == 1,
            "hard": hard(&items), "load": day_load(&items, &rt).round(), "status": st,
            "doneActs": da, "block": block, "week": wk, "deload": deload, "detail": det,
        }));
    }
    let changes: Vec<Value> = {
        let mut st = conn.prepare("SELECT id, ts, date, before, after, reason, origin, state, grp FROM plan_changes ORDER BY ts DESC, id DESC LIMIT 80").map_err(err)?;
        let v = st.query_map([], change_row).map_err(err)?.flatten().collect();
        v
    };
    // This week vs the headroom.
    let m = monday(&today);
    let week_plan: f64 = (0..7).filter_map(|k| row(conn, &add_days(&m, k))).map(|x| day_load(&x.0, &rt)).sum();
    let (block, _, deload) = phase(&start, &today, &md);
    let wk = ((day_num(&m).unwrap_or(0) - day_num(&start).unwrap_or(0)) / 7).max(0) % 4 + 1;
    let today_detail = row(conn, &today).map(|x| x.0).unwrap_or_default();
    let usual = if ctl > 0.0 { Some((ctl * 7.0).round()) } else { None };
    let apply = if auto_apply(conn) { "auto" } else { "ask" };
    let proposals: Vec<Value> = changes.iter().filter(|c| c["state"] == "proposed").cloned().collect();
    let tpl_v: Vec<Value> = tpl.iter().map(|d| items_json(d)).collect();
    let kinds: Vec<Value> = KINDS.iter().map(|k| json!({"key": k.0, "title": k.1, "hard": k.3})).collect();
    Ok(json!({
        "today": today, "days": days,
        "template": tpl_v, "templateAuto": auto,
        "phase": {"name": phase_name(&md), "block": block, "week": wk, "deload": deload},
        "week": {"plan": week_plan.round(), "cap": cap_week.map(|x| x.round()), "usual": usual},
        "changes": changes,
        "proposals": proposals,
        "applyMode": apply,
        "todayText": items_text(&today_detail),
        "kinds": kinds,
        "rates": rt,
    }))
}

/// The plan as text lines for the Claude package (7 days from today).
pub fn lines(conn: &Connection) -> Vec<String> {
    let today = engine_svc::today(&engine_svc::profile(conn));
    let mut out = Vec::new();
    for k in 0..7 {
        let d = add_days(&today, k);
        if let Some((items, src, _)) = row(conn, &d) {
            out.push(format!("{} ({}): {}{}", date_ru(&d), d, items_text(&items), if src == "template" { "" } else { " — изменено" }));
        }
    }
    let props: Vec<String> = conn
        .prepare("SELECT date, before, after, reason FROM plan_changes WHERE state = 'proposed' ORDER BY date")
        .ok()
        .map(|mut st| {
            let v: Vec<String> = st
                .query_map([], |r| {
                    let b: String = r.get(1)?;
                    let a: String = r.get(2)?;
                    Ok(format!(
                        "Предложение Kouros на {}: {} → {} ({})",
                        date_ru(&r.get::<_, String>(0)?),
                        items_text(&items_of(&serde_json::from_str(&b).unwrap_or(Value::Null))),
                        items_text(&items_of(&serde_json::from_str(&a).unwrap_or(Value::Null))),
                        r.get::<_, String>(3)?.split(" [").next().unwrap_or("")
                    ))
                })
                .map(|r| r.flatten().collect())
                .unwrap_or_default();
            v
        })
        .unwrap_or_default();
    out.extend(props);
    out
}

// ---------- the athlete's choice of the day (0.23) ----------

struct Ctx {
    s: Value,
    today: String,
    rt: BTreeMap<&'static str, f64>,
    cap_week: Option<f64>,
    t: f64,
    loads: BTreeMap<String, f64>,
    usual_day: f64,
}

fn ctx(conn: &mut Connection) -> Result<Ctx, String> {
    let s = engine_svc::summary(conn)?;
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let rest = engine_svc::resting_hr(conn, &p);
    let acts = engine_svc::refresh_metrics(conn, &p, rest)?;
    let rt = rates(&acts, &today, p.tz_offset_s);
    let ctl = s["load"]["ctl"].as_f64().unwrap_or(0.0);
    let cap_week = if ctl > 5.0 { Some(ctl * 7.0 * p.week_cap) } else { None };
    let cp = coach_svc::profile(conn);
    let t = cp["time"].as_str().and_then(|x| x.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(60.0);
    let mut loads: BTreeMap<String, f64> = BTreeMap::new();
    for a in &acts {
        *loads.entry(day_of(a.start_utc, p.tz_offset_s)).or_default() += a.load;
    }
    let usual_day = s["day"]["usualLoad"].as_f64().unwrap_or(0.0);
    Ok(Ctx { s, today, rt, cap_week, t, loads, usual_day })
}

/// Was a day hard: a hard planned day that was done, or a load well above the usual day.
fn was_hard(conn: &Connection, c: &Ctx, d: &str) -> bool {
    let load = c.loads.get(d).copied().unwrap_or(0.0);
    if c.usual_day > 0.0 && load >= c.usual_day * 1.4 {
        return true;
    }
    if d >= c.today.as_str() {
        return row(conn, d).map(|x| hard(&x.0)).unwrap_or(false);
    }
    load > 0.0 && row(conn, d).map(|x| hard(&x.0)).unwrap_or(false)
}

/// A lighter version of a choice: no hard items, runs shorter; with leg pain — no running.
fn lighter(items: &[Item], leg_pain: bool) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for i in items {
        let n = match i.kind.as_str() {
            "tempo" | "intervals" | "test" => Item { kind: "easy".into(), min: r5((i.min * 0.8).clamp(30.0, 50.0)) },
            "long" => Item { kind: "easy".into(), min: r5((i.min * 0.6).clamp(35.0, 55.0)) },
            "gym" => Item { kind: "ofp".into(), min: 30.0 },
            _ => i.clone(),
        };
        let n = if leg_pain && is_run(&n.kind) { Item { kind: "bike".into(), min: r5(n.min.max(30.0)) } } else { n };
        if !out.iter().any(|x| x.kind == n.kind) {
            out.push(n);
        }
    }
    if out.is_empty() {
        out.push(Item { kind: "walk".into(), min: 30.0 });
    }
    out
}

/// Checks a choice for a day: warnings with reasons and a lighter suggestion when it is risky.
pub fn check_choice(conn: &mut Connection, d: &str, items: &Value) -> Result<Value, String> {
    let c = ctx(conn)?;
    let chosen = items_of(items);
    let mut warn: Vec<String> = Vec::new();
    let is_hard = hard(&chosen) || chosen.iter().any(|i| i.kind == "gym");
    let mut suggestion: Option<Vec<Item>> = None;
    // Readiness (today only).
    if d == c.today {
        if let Some(allowed) = c.s["day"]["advice"]["tier"].as_str().and_then(|x| crate::engine::daily::TIERS.iter().position(|t| *t == x)) {
            if need_tier(&chosen) > allowed {
                let reasons: Vec<String> = c.s["day"]["advice"]["reasons"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                warn.push(format!(
                    "Сегодня тело готово на «{}»{}{}.",
                    c.s["day"]["advice"]["title"].as_str().unwrap_or(""),
                    c.s["readiness"]["score"].as_f64().map(|x| format!(" (готовность {x:.0})")).unwrap_or_default(),
                    if reasons.is_empty() { String::new() } else { format!(": {}", reasons.join("; ")) }
                ));
                suggestion = Some(fit(&chosen, allowed, c.t));
            }
        }
    }
    // Hard days in a row.
    if is_hard {
        let y1 = was_hard(conn, &c, &add_days(d, -1));
        let y2 = y1 && was_hard(conn, &c, &add_days(d, -2));
        if y2 {
            warn.push("Это третий тяжёлый день подряд — так растёт риск перегруза и травмы.".into());
        } else if y1 {
            warn.push("Вчера уже был тяжёлый день — двум подряд лучше не быть, мышцам и связкам нужно 36–48 часов.".into());
        }
    }
    // Acute:chronic load.
    if is_hard {
        let h = c.s["load"]["acwr"].as_f64().unwrap_or(0.0);
        let m = c.s["load"]["muscle"]["acwr"].as_f64().unwrap_or(0.0);
        if h > 1.3 || m > 1.3 {
            let (what, v) = if m > h { ("мышц и связок", m) } else { ("сердца", h) };
            warn.push(format!("Нагрузка {what} за неделю уже на {}% выше обычной — зона риска.", ((v - 1.0) * 100.0).round()));
        }
    }
    // The week vs the headroom.
    if let Some(cap) = c.cap_week.filter(|x| *x > 50.0) {
        let m = monday(d);
        let mut total = 0.0;
        for k in 0..7 {
            let e = add_days(&m, k);
            total += if e.as_str() < c.today.as_str() {
                c.loads.get(&e).copied().unwrap_or(0.0)
            } else if e == d {
                day_load(&chosen, &c.rt)
            } else {
                row(conn, &e).map(|x| day_load(&x.0, &c.rt)).unwrap_or(0.0)
            };
        }
        if total > cap * 1.05 {
            warn.push(format!("С этим выбором неделя выйдет ≈{} при безопасных {} — остальные дни придётся заметно облегчить.", total.round(), cap.round()));
        }
    }
    // Pain.
    let leg_pain = crate::body_svc::active(conn).iter().any(|p| p.2 >= 4 && p.3);
    if leg_pain && chosen.iter().any(|i| is_run(&i.kind)) {
        let names: Vec<String> = crate::body_svc::active(conn).into_iter().filter(|p| p.2 >= 4 && p.3).map(|p| format!("{} {}/10", p.1, p.2)).collect();
        warn.push(format!("Болит {} — бег через боль меняет технику и может превратить её в травму.", names.join(", ").to_lowercase()));
    }
    // Illness signals.
    let ill: Vec<String> = c.s["signals"].as_array().map(|a| a.iter().filter(|g| matches!(g["code"].as_str(), Some("rhr_up" | "temp_up" | "low_rhr_low_hrv" | "ill"))).filter_map(|g| g["text"].as_str().map(String::from)).collect()).unwrap_or_default();
    if is_hard && !ill.is_empty() {
        warn.push(format!("Есть признаки, что организм с чем-то борется: {}.", ill.join("; ")));
    }
    if !warn.is_empty() && suggestion.is_none() {
        suggestion = Some(lighter(&chosen, leg_pain));
    }
    let suggestion = suggestion.filter(|x| *x != chosen);
    Ok(json!({
        "ok": warn.is_empty(), "warnings": warn,
        "suggestion": suggestion.as_ref().map(|x| items_json(x)), "suggestionText": suggestion.as_ref().map(|x| items_text(x)),
    }))
}

/// «Подтверждаю»: today's plan stays as it is.
pub fn confirm(conn: &Connection, d: &str) -> Result<(), String> {
    conn.execute("UPDATE plan_days SET confirmed = 1 WHERE date = ?1", params![d]).map_err(err)?;
    Ok(())
}

/// The athlete's choice for a day: saved as is (the last word is theirs), then the following days
/// are rebalanced — no hard day right after a hard choice, a dropped key workout moves to a later
/// easy day, the week is trimmed to the headroom. Returns the changes made.
pub fn choose(conn: &mut Connection, d: &str, items: &Value) -> Result<Value, String> {
    let c = ctx(conn)?;
    let chosen = items_of(items);
    if chosen.is_empty() {
        return Err("Выбери хотя бы одно занятие или отдых".into());
    }
    let before = row(conn, d).map(|x| x.0).unwrap_or_default();
    put(conn, d, &chosen, "chosen", "")?;
    conn.execute("UPDATE plan_days SET confirmed = 1 WHERE date = ?1", params![d]).map_err(err)?;
    if before == chosen {
        return Ok(json!({"changes": []}));
    }
    add_change(conn, d, &before, &chosen, "выбрано тобой", "manual", "accepted", None)?;
    let tag = format!("[choice:{d}]");
    // Earlier rebalancing for this day is replaced.
    let mut changes: Vec<Value> = Vec::new();
    let mut apply = |conn: &Connection, e: &str, b: &[Item], a: &[Item], why: &str| -> Result<(), String> {
        if b == a {
            return Ok(());
        }
        add_change(conn, e, b, a, &format!("{why} {tag}"), "kouros", "accepted", None)?;
        put(conn, e, a, "accepted", "")?;
        changes.push(json!({"date": e, "dateRu": date_ru(e), "before": items_text(b), "after": items_text(a), "why": why}));
        Ok(())
    };
    let chosen_hard = hard(&chosen);
    let lost_key: Vec<Item> = before.iter().filter(|i| kind(&i.kind).3 && !chosen.iter().any(|x| x.kind == i.kind)).cloned().collect();
    // 1. A hard choice: tomorrow must not be hard as well — its key workout moves on.
    let mut carry: Vec<Item> = lost_key.clone();
    if chosen_hard {
        let n1 = add_days(d, 1);
        if let Some((ni, ns, _)) = row(conn, &n1) {
            if hard(&ni) && ns != "chosen" {
                let key: Vec<Item> = ni.iter().filter(|i| kind(&i.kind).3).cloned().collect();
                let after = lighter(&ni, false);
                apply(conn, &n1, &ni, &after, &format!("после тяжёлого {} — лёгкий день", date_ru(d)))?;
                for k in key {
                    if !carry.iter().any(|x| x.kind == k.kind) && !chosen.iter().any(|x| x.kind == k.kind) {
                        carry.push(k);
                    }
                }
            }
        }
    }
    // 2. Key workouts that lost their day go to the next easy day in 2–4 days (not next to a hard one).
    if !carry.is_empty() {
        let start = if chosen_hard { 2 } else { 1 };
        for j in start..=4 {
            let e = add_days(d, j);
            let Some((ei, es, _)) = row(conn, &e) else { break };
            if es == "chosen" || hard(&ei) || ei.iter().any(|i| i.kind == "gym") {
                continue;
            }
            let prev_hard = j == 1 && chosen_hard || row(conn, &add_days(&e, -1)).map(|x| hard(&x.0)).unwrap_or(false);
            let next_hard = row(conn, &add_days(&e, 1)).map(|x| hard(&x.0)).unwrap_or(false);
            if prev_hard || next_hard {
                continue;
            }
            let mut moved = carry.clone();
            moved.extend(ei.iter().filter(|i| matches!(i.kind.as_str(), "ofp" | "mobility")).cloned());
            apply(conn, &e, &ei, &moved, &format!("перенос ключевой тренировки с {}", date_ru(d)))?;
            break;
        }
    }
    // 3. The rest of the week vs the headroom.
    if let Some(cap) = c.cap_week.filter(|x| *x > 50.0) {
        let m = monday(d);
        let days: Vec<String> = (0..7).map(|k| add_days(&m, k)).collect();
        let mut done = 0.0;
        let mut rest: Vec<(String, Vec<Item>, String)> = Vec::new();
        for e in &days {
            if e.as_str() < c.today.as_str() {
                done += c.loads.get(e).copied().unwrap_or(0.0);
            } else if let Some((i, src, _)) = row(conn, e) {
                if e.as_str() <= d || src == "chosen" {
                    done += day_load(&i, &c.rt);
                } else {
                    rest.push((e.clone(), i, src));
                }
            }
        }
        let planned: f64 = rest.iter().map(|x| day_load(&x.1, &c.rt)).sum();
        if done + planned > cap * 1.05 && planned > 0.0 {
            let flex: f64 = rest.iter().flat_map(|x| x.1.iter()).filter(|i| matches!(i.kind.as_str(), "easy" | "long" | "bike" | "walk")).map(|i| i.min * c.rt.get(i.kind.as_str()).copied().unwrap_or(0.5)).sum();
            if flex > 0.0 {
                let k = (1.0 - (done + planned - cap) / flex).clamp(0.3, 0.95);
                for (e, items, _) in &rest {
                    let after: Vec<Item> = items.iter().map(|i| if matches!(i.kind.as_str(), "easy" | "long" | "bike" | "walk") { Item { kind: i.kind.clone(), min: r5(i.min * k).max(20.0) } } else { i.clone() }).collect();
                    apply(conn, e, items, &after, &format!("неделя ≈{} при безопасных {} — день короче", (done + planned).round(), cap.round()))?;
                }
            }
        }
    }
    Ok(json!({"changes": changes}))
}
