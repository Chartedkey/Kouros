//! «Витамины и лекарства»: your own list of supplements and medicines with their composition
//! (from the label), intake marked by hand only, daily totals from supplements and safe upper
//! limits. YAZIO supplement entries are never counted — only what you mark here.

use crate::engine::add_days;
use crate::store;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Tolerable upper intake levels for an adult (from all sources, unless noted), approx. EFSA/NASEM.
/// Metric → (limit, note).
pub const UL: [(&str, f64, &str); 13] = [
    ("dietary_vitamin_a_total", 3000.0, "ретинол"),
    ("dietary_vitamin_d_total", 100.0, ""),
    ("dietary_vitamin_e_total", 300.0, ""),
    ("dietary_vitamin_b6_total", 25.0, "EFSA"),
    ("dietary_folate_total", 1000.0, "из добавок"),
    ("dietary_niacin_total", 35.0, "из добавок"),
    ("dietary_calcium_total", 2500.0, ""),
    ("dietary_iron_total", 45.0, ""),
    ("dietary_magnesium_total", 350.0, "из добавок"),
    ("dietary_zinc_total", 25.0, "EFSA"),
    ("dietary_selenium_total", 255.0, "EFSA"),
    ("dietary_copper_total", 5.0, "EFSA"),
    ("dietary_manganese_total", 11.0, ""),
];

/// UL counted only against what supplements bring (food is not counted for these).
fn ul_supp_only(metric: &str) -> bool {
    matches!(metric, "dietary_folate_total" | "dietary_niacin_total" | "dietary_magnesium_total")
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let comp: String = r.get(6)?;
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "name": r.get::<_, String>(2)?,
        "form": r.get::<_, Option<String>>(3)?, "dose": r.get::<_, f64>(4)?, "schedule": r.get::<_, String>(5)?,
        "components": serde_json::from_str::<Value>(&comp).unwrap_or(json!([])),
        "courseFrom": r.get::<_, Option<String>>(7)?, "courseTo": r.get::<_, Option<String>>(8)?,
        "notes": r.get::<_, Option<String>>(9)?, "archived": r.get::<_, i64>(10)? == 1,
        "extra": r.get::<_, Option<String>>(11)?.and_then(|e| serde_json::from_str::<Value>(&e).ok()).unwrap_or(json!({})),
    }))
}

const COLS: &str = "id, kind, name, form, dose, schedule, components, course_from, course_to, notes, archived, extra";

pub fn list(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM meds ORDER BY archived, kind DESC, name")).map_err(err)?;
    let v = st.query_map([], row).map_err(err)?.flatten().collect();
    Ok(v)
}

/// Create or update one preparation. Components: [{key, name, amount, unit}] where key is a
/// nutrient metric (dietary_*_total, amounts already in its unit) or empty for anything else.
pub fn save(conn: &Connection, v: &Value) -> Result<i64, String> {
    let name = v["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or("Нужно название")?;
    let kind = match v["kind"].as_str() {
        Some("medicine") => "medicine",
        _ => "supplement",
    };
    let mut comps = v["components"].as_array().cloned().unwrap_or_default();
    for c in comps.iter_mut() {
        if c["amount"].as_f64().map(|a| a < 0.0).unwrap_or(true) {
            return Err("Количество компонента — число".into());
        }
        // A component typed by hand: recognise it («холекальциферол» → vitamin D) when the unit fits.
        if c["key"].as_str().unwrap_or("").is_empty() {
            let name = c["name"].as_str().unwrap_or("").to_string();
            let unit = c["unit"].as_str().unwrap_or("").to_string();
            let fixed = crate::catalog_svc::component(&name, c["amount"].as_f64().unwrap_or(0.0), &unit);
            if !fixed["key"].as_str().unwrap_or("").is_empty() {
                *c = fixed;
            }
        }
    }
    // Medicine: active ingredient and sport-relevant group (from the box name when not given).
    let mut extra = if v["extra"].is_object() { v["extra"].clone() } else { json!({}) };
    if kind == "medicine" {
        let inn = extra["inn"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from).or_else(|| crate::drugs::inn_of_trade(name).map(String::from));
        if let Some(i) = &inn {
            extra["inn"] = json!(i);
        }
        if extra["group"].as_str().map(|g| g.is_empty()).unwrap_or(true) {
            let g = inn.as_deref().and_then(crate::drugs::group_of_inn).or_else(|| crate::drugs::group_of_inn(name));
            extra["group"] = json!(g);
        }
    }
    let extra_s = extra.to_string();
    let dose = v["dose"].as_f64().filter(|d| *d > 0.0).unwrap_or(1.0);
    let schedule = v["schedule"].as_str().unwrap_or("daily").to_string();
    let comps_s = Value::Array(comps).to_string();
    let from = v["courseFrom"].as_str().filter(|s| !s.is_empty()).map(String::from);
    let to = v["courseTo"].as_str().filter(|s| !s.is_empty()).map(String::from);
    let form = v["form"].as_str().map(String::from);
    let notes = v["notes"].as_str().map(String::from);
    let archived = if v["archived"].as_bool() == Some(true) { 1 } else { 0 };
    if let Some(id) = v["id"].as_i64() {
        conn.execute(
            "UPDATE meds SET kind = ?1, name = ?2, form = ?3, dose = ?4, schedule = ?5, components = ?6, course_from = ?7, course_to = ?8, notes = ?9, archived = ?10, extra = ?12 WHERE id = ?11",
            params![kind, name, form, dose, schedule, comps_s, from, to, notes, archived, id, extra_s],
        )
        .map_err(err)?;
        Ok(id)
    } else {
        conn.execute(
            "INSERT INTO meds (kind, name, form, dose, schedule, components, course_from, course_to, notes, archived, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![kind, name, form, dose, schedule, comps_s, from, to, notes, archived, extra_s],
        )
        .map_err(err)?;
        Ok(conn.last_insert_rowid())
    }
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM med_log WHERE med_id = ?1", params![id]).map_err(err)?;
    conn.execute("DELETE FROM meds WHERE id = ?1", params![id]).map_err(err)?;
    Ok(())
}

/// Marks a preparation taken on `date` (servings), or removes the mark when servings is 0.
/// `ts` = when it was taken (default: now, or the old time when only servings change).
pub fn take(conn: &Connection, id: i64, date: &str, servings: f64, ts: Option<i64>) -> Result<(), String> {
    let old: Option<i64> = conn.query_row("SELECT ts FROM med_log WHERE med_id = ?1 AND date = ?2", params![id, date], |r| r.get(0)).ok();
    conn.execute("DELETE FROM med_log WHERE med_id = ?1 AND date = ?2", params![id, date]).map_err(err)?;
    if servings > 0.0 {
        let t = ts.or(old).unwrap_or_else(store::now);
        conn.execute("INSERT INTO med_log (med_id, date, ts, servings) VALUES (?1, ?2, ?3, ?4)", params![id, date, t, servings])
            .map_err(err)?;
    }
    Ok(())
}

/// Kouros keys that are food rather than micronutrients: counted only when the product's
/// «учитывать в питании» is on (isotonics by default; protein is usually already in YAZIO).
const FOODY: [&str; 7] = ["dietary_energy_total", "dietary_protein_total", "dietary_carbohydrates_total", "dietary_fat_total_total", "dietary_fiber_total", "water_ml", "dietary_water_total"];

/// Does this component of this preparation go into the day's totals?
fn counts(m: &Value, key: &str) -> bool {
    if key == "dietary_caffeine_total" {
        return true; // caffeine from a pre-workout always counts toward the day's caffeine
    }
    match m["extra"]["countFood"].as_bool() {
        Some(true) => true,
        // Off: the product is already in the food diary (or the athlete does not want it counted).
        Some(false) if matches!(m["extra"]["catalogKind"].as_str(), Some("protein") | Some("isotonic")) => false,
        _ => !FOODY.contains(&key),
    }
}

/// Servings taken per (date, med id) from `from`.
fn log(conn: &Connection, from: &str) -> BTreeMap<(String, i64), f64> {
    let mut out = BTreeMap::new();
    if let Ok(mut st) = conn.prepare("SELECT date, med_id, servings FROM med_log WHERE date >= ?1") {
        let rows: Vec<(String, i64, f64)> = st.query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
        for (d, id, s) in rows {
            *out.entry((d, id)).or_insert(0.0) += s;
        }
    }
    out
}

/// Nutrients from marked supplements per day: date → metric → amount.
pub fn supp_by_day(conn: &Connection, from: &str) -> BTreeMap<String, BTreeMap<String, f64>> {
    let meds = list(conn).unwrap_or_default();
    let by_id: BTreeMap<i64, &Value> = meds.iter().filter_map(|m| m["id"].as_i64().map(|i| (i, m))).collect();
    let mut out: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for ((d, id), servings) in log(conn, from) {
        let Some(m) = by_id.get(&id) else { continue };
        for c in m["components"].as_array().cloned().unwrap_or_default() {
            if let (Some(k), Some(a)) = (c["key"].as_str().filter(|k| !k.is_empty()), c["amount"].as_f64()) {
                if !counts(m, k) {
                    continue;
                }
                let k = if k == "water_ml" { "dietary_water_total" } else { k };
                *out.entry(d.clone()).or_default().entry(k.to_string()).or_insert(0.0) += a * servings;
            }
        }
        // Isotonic powder mixed with water: the drink's water counts too.
        if m["extra"]["countFood"].as_bool() == Some(true) {
            if let Some(w) = m["extra"]["waterMl"].as_f64().filter(|w| *w > 0.0) {
                *out.entry(d.clone()).or_default().entry("dietary_water_total".into()).or_insert(0.0) += w * servings;
            }
        }
    }
    out
}

/// What supplements add to a daily food series (energy, macros, water, caffeine): date → amount.
pub fn food_add(conn: &Connection, metric: &str) -> BTreeMap<String, f64> {
    if !matches!(metric, "dietary_energy_total" | "dietary_protein_total" | "dietary_carbohydrates_total" | "dietary_fat_total_total" | "dietary_water_total" | "dietary_caffeine_total") {
        return BTreeMap::new();
    }
    let has: i64 = conn.query_row("SELECT count(*) FROM med_log", [], |r| r.get(0)).unwrap_or(0);
    if has == 0 {
        return BTreeMap::new();
    }
    supp_by_day(conn, "2000-01-01").into_iter().filter_map(|(d, m)| m.get(metric).copied().filter(|v| *v > 0.0).map(|v| (d, v))).collect()
}

/// What was taken on one day (for the Health day card).
pub fn taken_on(conn: &Connection, date: &str) -> Vec<Value> {
    let meds = list(conn).unwrap_or_default();
    let lg = log(conn, date);
    meds.iter()
        .filter_map(|m| {
            let id = m["id"].as_i64()?;
            let s = lg.get(&(date.to_string(), id))?;
            Some(json!({"id": id, "name": m["name"], "kind": m["kind"], "servings": s}))
        })
        .collect()
}

/// The section: list with today's marks, 14-day marks, totals today and 7-day UL check.
/// `food7` = average daily intake from food over the last 7 days, by metric (for the UL check).
pub fn summary(conn: &Connection, today: &str, food7: &BTreeMap<String, f64>) -> Result<Value, String> {
    let meds = list(conn)?;
    let from14 = add_days(today, -13);
    let lg = log(conn, &from14);
    let supp = supp_by_day(conn, &add_days(today, -6));
    let mut supp7: BTreeMap<String, f64> = BTreeMap::new();
    for m in supp.values() {
        for (k, v) in m {
            *supp7.entry(k.clone()).or_insert(0.0) += v / 7.0;
        }
    }
    let eff = effects(conn, today, &meds);
    let items: Vec<Value> = meds
        .iter()
        .map(|m| {
            let id = m["id"].as_i64().unwrap_or(0);
            let days: Vec<Value> = (0..14)
                .rev()
                .map(|k| {
                    let d = add_days(today, -k);
                    json!([d.clone(), lg.get(&(d, id)).copied().unwrap_or(0.0)])
                })
                .collect();
            let in_course = match (m["courseFrom"].as_str(), m["courseTo"].as_str()) {
                (Some(a), Some(b)) => today >= a && today <= b,
                (Some(a), None) => today >= a,
                (None, Some(b)) => today <= b,
                _ => true,
            };
            let mut o = m.clone();
            o["today"] = json!(lg.get(&(today.to_string(), id)).copied().unwrap_or(0.0));
            o["days"] = json!(days);
            o["active"] = json!(!m["archived"].as_bool().unwrap_or(false) && in_course);
            o["tips"] = json!(tips(m));
            o["effect"] = eff.get(&id).cloned().unwrap_or(Value::Null);
            o["groupInfo"] = m["extra"]["group"].as_str().and_then(crate::drugs::group).map(|g| json!({"key": g.0, "name": g.1, "note": g.2})).unwrap_or(Value::Null);
            o
        })
        .collect();
    let warnings: Vec<Value> = UL
        .iter()
        .filter_map(|(k, lim, note)| {
            let s = supp7.get(*k).copied().unwrap_or(0.0);
            if s <= 0.0 {
                return None;
            }
            let total = if ul_supp_only(k) { s } else { s + food7.get(*k).copied().unwrap_or(0.0) };
            (total > *lim).then(|| json!({"key": k, "avg": (total * 10.0).round() / 10.0, "supp": (s * 10.0).round() / 10.0, "limit": lim, "note": note}))
        })
        .collect();
    let today_tot: Vec<Value> = supp.get(today).map(|m| m.iter().map(|(k, v)| json!([k, (v * 100.0).round() / 100.0])).collect()).unwrap_or_default();
    let unknown: Vec<String> = meds
        .iter()
        .filter(|m| !m["archived"].as_bool().unwrap_or(false))
        .flat_map(|m| m["components"].as_array().cloned().unwrap_or_default())
        .filter(|c| c["key"].as_str().unwrap_or("").is_empty())
        .filter_map(|c| c["name"].as_str().map(String::from))
        .collect();
    let lg_today: BTreeMap<i64, i64> = marks(conn, today).into_iter().filter(|x| x.date == today).map(|x| (x.id, x.ts)).collect();
    let tz = crate::engine_svc::profile(conn).tz_offset_s;
    let items: Vec<Value> = items
        .into_iter()
        .map(|mut o| {
            if let Some(t) = o["id"].as_i64().and_then(|i| lg_today.get(&i)) {
                o["todayTime"] = json!(hhmm(*t, tz));
            }
            o
        })
        .collect();
    Ok(json!({"items": items, "today": today_tot, "supp7": supp7, "warnings": warnings,
              "timing": timing(conn, today, &meds), "notes": med_notes(conn, today, &meds), "unknown": unknown}))
}

// ---------- 0.18: timing, interactions, medicine groups, personal effect ----------

fn has(m: &Value, key: &str) -> Option<f64> {
    m["components"].as_array()?.iter().find(|c| c["key"].as_str() == Some(key)).and_then(|c| c["amount"].as_f64())
}

const FAT_SOLUBLE: [&str; 6] = ["dietary_vitamin_a_total", "dietary_vitamin_d_total", "dietary_vitamin_e_total", "dietary_vitamin_k_total", "epa", "dha"];

/// Short how-to-take tips for one preparation, from its components.
pub fn tips(m: &Value) -> Vec<String> {
    let mut t = Vec::new();
    if has(m, "dietary_iron_total").is_some() {
        t.push("Железо — лучше натощак или с витамином C; кальций, цинк, кофе и чай — не ближе 2 часов.".to_string());
    }
    if FAT_SOLUBLE.iter().any(|k| has(m, k).is_some()) {
        t.push("Жирорастворимые (A, D, E, K, омега-3) усваиваются с едой, где есть жир.".into());
    }
    if has(m, "dietary_calcium_total").map(|a| a > 500.0).unwrap_or(false) {
        t.push("Кальций больше 500 мг лучше делить на два приёма.".into());
    }
    if has(m, "dietary_zinc_total").map(|a| a >= 25.0).unwrap_or(false) {
        t.push("Цинк в большой дозе мешает магнию и меди — не в один приём с магнием.".into());
    }
    if has(m, "dietary_magnesium_total").is_some() {
        t.push("Магний удобно вечером; не в один приём с железом и большой дозой цинка.".into());
    }
    if has(m, "dietary_caffeine_total").is_some() {
        t.push("Кофеин — не позже чем за 8 часов до сна.".into());
    }
    if has(m, "melatonin").is_some() {
        t.push("Мелатонин — за 30–60 минут до сна; утренняя готовность в такие дни может выглядеть иначе.".into());
    }
    if has(m, "creatine").is_some() {
        t.push("Креатин работает от регулярности, время приёма почти не важно.".into());
    }
    if let Some(g) = m["extra"]["group"].as_str().and_then(crate::drugs::group) {
        t.push(format!("{}: {} Если сомневаешься — обсуди с врачом.", g.1, g.2));
    }
    t
}

struct Mark {
    id: i64,
    date: String,
    ts: i64,
}

fn marks(conn: &Connection, from: &str) -> Vec<Mark> {
    let Ok(mut st) = conn.prepare("SELECT med_id, date, ts FROM med_log WHERE date >= ?1 AND servings > 0 ORDER BY ts") else { return Vec::new() };
    st.query_map(params![from], |r| Ok(Mark { id: r.get(0)?, date: r.get(1)?, ts: r.get(2)? })).map(|r| r.flatten().collect()).unwrap_or_default()
}

fn hhmm(ts: i64, tz: i64) -> String {
    let m = (ts + tz).rem_euclid(86400) / 60;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Timing over the last 7 days, from the moments you marked «Принять» and the food diary:
/// iron next to calcium / zinc / coffee / tea, fat-soluble without a meal, late caffeine,
/// magnesium with a big dose of zinc.
pub fn timing(conn: &Connection, today: &str, meds: &[Value]) -> Vec<Value> {
    let p = crate::engine_svc::profile(conn);
    let tz = p.tz_offset_s;
    let from = add_days(today, -6);
    let mk = marks(conn, &from);
    if mk.is_empty() {
        return Vec::new();
    }
    let by_id: BTreeMap<i64, &Value> = meds.iter().filter_map(|m| m["id"].as_i64().map(|i| (i, m))).collect();
    let food = crate::food_est::entries(conn, &from, tz);
    let meals: Vec<i64> = food.iter().filter(|(_, e)| e.kcal >= 120.0 && e.kind != "supplement").map(|(_, e)| e.ts).collect();
    let mut drinks: Vec<i64> = food
        .iter()
        .filter(|(_, e)| {
            let n = e.name.to_lowercase();
            ["кофе", "coffee", "эспрессо", "капучино", "латте", "американо", "чай", "tea"].iter().any(|w| n.contains(w))
        })
        .map(|(_, e)| e.ts)
        .collect();
    if let Ok(mut st) = conn.prepare("SELECT ts FROM hm WHERE metric = 'food_entry_caffeine' AND date >= ?1 AND num > 0") {
        let v: Vec<String> = st.query_map(params![from], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
        drinks.extend(v.iter().filter_map(|t| crate::health_svc::iso(t)));
    }
    let near = |a: i64, b: i64, h: i64| (a - b).abs() <= h * 3600;
    let mut out: BTreeMap<String, (String, String, Vec<String>)> = BTreeMap::new();
    let mut push = |code: String, title: String, text: String, day: &str| {
        let e = out.entry(code).or_insert((title, text, Vec::new()));
        if !e.2.contains(&day.to_string()) {
            e.2.push(day.to_string());
        }
    };
    for a in &mk {
        let Some(m) = by_id.get(&a.id) else { continue };
        let name = m["name"].as_str().unwrap_or("");
        if has(m, "dietary_iron_total").is_some() {
            for b in mk.iter().filter(|b| b.id != a.id && near(a.ts, b.ts, 2)) {
                let Some(o) = by_id.get(&b.id) else { continue };
                if has(o, "dietary_calcium_total").map(|c| c >= 100.0).unwrap_or(false) || has(o, "dietary_zinc_total").map(|z| z >= 15.0).unwrap_or(false) {
                    push(format!("iron_min:{}:{}", a.id, b.id), "Железо и кальций/цинк вместе".into(),
                         format!("{name} и {} приняты ближе 2 часов — они мешают друг другу всасываться. Разнеси приёмы.", o["name"].as_str().unwrap_or("")), &a.date);
                }
            }
            if drinks.iter().any(|t| near(a.ts, *t, 2)) {
                push(format!("iron_cof:{}", a.id), "Железо и кофе/чай".into(), format!("{name} рядом с кофе или чаем (±2 ч) — железа усвоится заметно меньше."), &a.date);
            }
        }
        if FAT_SOLUBLE.iter().any(|k| has(m, k).is_some()) && !meals.iter().any(|t| near(a.ts, *t, 1)) && !food.is_empty() {
            push(format!("fat:{}", a.id), "Жирорастворимые без еды".into(), format!("{name} принят без еды рядом (±1 ч) — A, D, E, K и омега-3 лучше с едой, где есть жир."), &a.date);
        }
        if has(m, "dietary_caffeine_total").map(|c| c >= 40.0).unwrap_or(false) && (a.ts + tz).rem_euclid(86400) / 60 >= 16 * 60 {
            push(format!("caf:{}", a.id), "Кофеин поздно".into(), format!("{name} в {} — кофеин после 16:00 может сократить глубокий сон.", hhmm(a.ts, tz)), &a.date);
        }
        if has(m, "dietary_magnesium_total").is_some() {
            for b in mk.iter().filter(|b| b.id != a.id && near(a.ts, b.ts, 1)) {
                if let Some(o) = by_id.get(&b.id).filter(|o| has(o, "dietary_zinc_total").map(|z| z >= 40.0).unwrap_or(false)) {
                    push(format!("mgzn:{}:{}", a.id, b.id), "Магний и цинк вместе".into(), format!("{name} и {} в один приём — большая доза цинка мешает магнию.", o["name"].as_str().unwrap_or("")), &a.date);
                }
            }
        }
    }
    out.into_iter()
        .map(|(code, (title, text, days))| json!({"code": code, "title": title, "text": text, "days": days.len(), "today": days.iter().any(|d| d == today), "last": days.last()}))
        .collect()
}

/// Was a preparation in use on `date` (marked within `back` days before, or a daily/course
/// preparation inside its dates)?
fn in_use(m: &Value, mk: &[Mark], date: &str, back: i64) -> bool {
    let id = m["id"].as_i64().unwrap_or(0);
    let from = add_days(date, -back);
    if mk.iter().any(|x| x.id == id && x.date.as_str() >= from.as_str() && x.date.as_str() <= date) {
        return true;
    }
    if m["archived"].as_bool().unwrap_or(false) || m["schedule"] == "asneeded" {
        return false;
    }
    let a = m["courseFrom"].as_str().unwrap_or("");
    let b = m["courseTo"].as_str().unwrap_or("9999");
    m["schedule"] == "course" && !a.is_empty() && date >= a && date <= b
}

/// Last day a fluoroquinolone still matters for tendons (last intake or course end + 14 days).
pub fn fq_until(conn: &Connection, today: &str) -> Option<(String, String)> {
    let meds = list(conn).ok()?;
    let mk = marks(conn, &add_days(today, -30));
    let mut best: Option<(String, String)> = None;
    for m in meds.iter().filter(|m| m["extra"]["group"] == "fluoroquinolone") {
        let id = m["id"].as_i64().unwrap_or(0);
        let mut last = mk.iter().filter(|x| x.id == id).map(|x| x.date.clone()).max();
        if in_use(m, &mk, today, 0) {
            let end = m["courseTo"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| today.to_string());
            last = Some(last.map(|l| l.max(end.clone())).unwrap_or(end));
        }
        if let Some(l) = last {
            let until = add_days(&l, 14);
            if until.as_str() >= today && best.as_ref().map(|b| until > b.0).unwrap_or(true) {
                best = Some((until, m["name"].as_str().unwrap_or("").to_string()));
            }
        }
    }
    best
}

/// What medicines in use mean for today: notes for «Сегодня», the coach and the Claude package.
pub fn med_notes(conn: &Connection, today: &str, meds: &[Value]) -> Vec<Value> {
    let mk = marks(conn, &add_days(today, -30));
    let yesterday = add_days(today, -1);
    let hard_today = crate::plan_svc::day_hard(conn, today) || crate::plan_svc::day_items_text(conn, today).to_lowercase().contains("длительн");
    let mut out = Vec::new();
    for m in meds {
        let Some((key, gname, _)) = m["extra"]["group"].as_str().and_then(crate::drugs::group) else { continue };
        let name = m["name"].as_str().unwrap_or("");
        let (level, text) = match key {
            "nsaid" if in_use(m, &mk, today, 1) && hard_today => (
                "warn",
                format!("{name} ({gname}) за последние сутки, а сегодня тяжёлая или длительная тренировка. Обезболивающее прячет сигнал боли и вместе с потерей воды нагружает почки — лучше не принимать «на всякий случай»; если болит, тренировку сделай лёгкой."),
            ),
            "nsaid" if in_use(m, &mk, today, 1) => ("info", format!("{name} ({gname}): боль приглушена — если принимаешь из-за неё, ориентируйся не на ощущения, а на план полегче.")),
            "fluoroquinolone" => match fq_until(conn, today) {
                Some((until, _)) => ("warn", format!("{name} ({gname}): до {} без прыжков, интервалов, темпа и длительных — риск для сухожилий, особенно ахилла. Боль в ахилле — сразу к врачу.", crate::plan_svc::date_ru(&until))),
                None => continue,
            },
            "beta_blocker" if in_use(m, &mk, today, 3) => ("info", format!("{name} ({gname}) снижает пульс — пульсовые зоны, нагрузка по пульсу и готовность сейчас неточны. Ориентируйся на ощущения (RPE); как тренироваться — обсуди с врачом.")),
            "antihistamine_sed" | "hypnotic" if mk.iter().any(|x| x.id == m["id"].as_i64().unwrap_or(0) && x.date == yesterday) => (
                "info",
                format!("Вчера {name} ({gname}) — сон и утренняя готовность могут выглядеть иначе, возможна заторможенность. Не спеши с выводами по одному утру."),
            ),
            "corticosteroid" | "statin" | "diuretic" if in_use(m, &mk, today, 3) => ("info", format!("{name} ({gname}): {}", crate::drugs::group(key).map(|g| g.2).unwrap_or(""))),
            _ => continue,
        };
        out.push(json!({"group": key, "groupName": gname, "med": name, "level": level, "text": text}));
    }
    // Melatonin last night — sleep may look different.
    for m in meds.iter().filter(|m| has(m, "melatonin").is_some()) {
        if mk.iter().any(|x| x.id == m["id"].as_i64().unwrap_or(0) && x.date == yesterday) {
            out.push(json!({"group": "melatonin", "groupName": "Мелатонин", "med": m["name"], "level": "info",
                            "text": format!("Вчера {} с мелатонином — учти это, глядя на засыпание и сон.", m["name"].as_str().unwrap_or(""))}));
        }
    }
    out
}

fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Personal effect over 120 days: next-morning HRV, resting pulse, sleep and readiness on days with
/// vs without the preparation (from its first mark), and 14 days before vs after a course start.
/// Needs ≥5 days on each side; a correlation, not proof.
pub fn effects(conn: &Connection, today: &str, meds: &[Value]) -> BTreeMap<i64, Value> {
    let mut out = BTreeMap::new();
    let mk = marks(conn, &add_days(today, -120));
    if meds.is_empty() {
        return out;
    }
    let hd = crate::engine_svc::health_data(conn);
    let ready: BTreeMap<String, f64> = conn
        .prepare("SELECT date, score FROM readiness_log WHERE score IS NOT NULL AND date >= ?1")
        .ok()
        .map(|mut st| {
            let v: Vec<(String, f64)> = st.query_map(params![add_days(today, -130)], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
            v.into_iter().collect()
        })
        .unwrap_or_default();
    // Outcome for a set of mornings: (hrv, rhr, sleep min, readiness) means and counts.
    let side = |days: &[String]| -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let (mut h, mut r, mut s, mut q) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for d in days {
            if let Some(v) = hd.hrv.get(d) {
                h.push(*v)
            }
            if let Some(v) = hd.rhr.get(d) {
                r.push(*v)
            }
            if let Some(v) = hd.sleep_s.get(d) {
                s.push(v / 60.0)
            }
            if let Some(v) = ready.get(d) {
                q.push(*v)
            }
        }
        (h, r, s, q)
    };
    let compare = |a: &[String], b: &[String]| -> Option<Value> {
        let (ha, ra, sa, qa) = side(a);
        let (hb, rb, sb, qb) = side(b);
        let ok = |x: &Vec<f64>, y: &Vec<f64>| x.len() >= 5 && y.len() >= 5;
        if !ok(&ha, &hb) && !ok(&sa, &sb) && !ok(&qa, &qb) {
            return None;
        }
        let pct = |x: &Vec<f64>, y: &Vec<f64>| if ok(x, y) { mean(y).filter(|m| *m > 0.0).and_then(|m| mean(x).map(|n| r1((n / m - 1.0) * 100.0))) } else { None };
        let diff = |x: &Vec<f64>, y: &Vec<f64>| if ok(x, y) { mean(x).and_then(|n| mean(y).map(|m| r1(n - m))) } else { None };
        Some(json!({"with": a.len(), "without": b.len(), "hrvPct": pct(&ha, &hb), "rhr": diff(&ra, &rb), "sleepMin": diff(&sa, &sb).map(f64::round), "ready": diff(&qa, &qb).map(f64::round)}))
    };
    for m in meds {
        let id = m["id"].as_i64().unwrap_or(0);
        let mine: std::collections::BTreeSet<String> = mk.iter().filter(|x| x.id == id).map(|x| x.date.clone()).collect();
        let mut v = json!({});
        if let Some(first) = mine.iter().next().cloned() {
            let mut with = Vec::new();
            let mut without = Vec::new();
            let mut d = first;
            while d.as_str() < today {
                let morning = add_days(&d, 1);
                if mine.contains(&d) {
                    with.push(morning)
                } else {
                    without.push(morning)
                }
                d = add_days(&d, 1);
            }
            if let Some(c) = compare(&with, &without) {
                v["nextMorning"] = c;
            }
        }
        if let Some(start) = m["courseFrom"].as_str().filter(|s| !s.is_empty() && add_days(s, 13).as_str() < today) {
            let before: Vec<String> = (1..=14).map(|k| add_days(start, -k)).collect();
            let after: Vec<String> = (0..14).map(|k| add_days(start, k)).collect();
            if let Some(c) = compare(&after, &before) {
                v["course"] = c;
            }
        }
        if v.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            out.insert(id, v);
        }
    }
    out
}
