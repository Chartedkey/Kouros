//! Exercise library (0.22): ~870 exercises from Free Exercise DB (public domain) with Russian
//! names, synonyms and a short how-to (`tools/exercises/build.py` → `resources/exercises.json.gz`,
//! photos in `public/ex/<id>.webp`). Used by the strength sets editor (search, card with a
//! two-frame animation), by the effective load of a set (own body weight × share, resistance
//! bands from «Инвентарь») and by the muscle load (groups from the exercise's muscles).

use crate::db;
use crate::engine::add_days;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{OnceLock, RwLock};

static LIB_GZ: &[u8] = include_bytes!("../resources/exercises.json.gz");
/// 1.4: Kouros' own exercises for the runner's ОФП list (no photos), merged into the library.
static EXTRA: &str = include_str!("../resources/exercises_extra.json");

fn extra() -> &'static Value {
    static E: OnceLock<Value> = OnceLock::new();
    E.get_or_init(|| serde_json::from_str(EXTRA).unwrap_or(Value::Null))
}

fn lib() -> &'static Value {
    static L: OnceLock<Value> = OnceLock::new();
    L.get_or_init(|| {
        let mut s = String::new();
        let mut d = flate2::read::GzDecoder::new(LIB_GZ);
        if d.read_to_string(&mut s).is_err() {
            return Value::Null;
        }
        let mut v: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
        if let (Some(items), Some(more)) = (v["items"].as_array_mut(), extra()["items"].as_array()) {
            items.extend(more.iter().cloned());
        }
        v
    })
}

/// Your own exercises (1.4, settings `custom_exercises`), kept in memory for the muscle load.
static CUSTOM: RwLock<Vec<Value>> = RwLock::new(Vec::new());

pub fn load_custom(conn: &Connection) {
    let v: Vec<Value> = db::get_setting(conn, "custom_exercises").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Ok(mut c) = CUSTOM.write() {
        *c = v;
    }
}

pub fn custom_snapshot() -> Vec<Value> {
    CUSTOM.read().map(|c| c.clone()).unwrap_or_default()
}

fn index() -> &'static HashMap<String, usize> {
    static I: OnceLock<HashMap<String, usize>> = OnceLock::new();
    I.get_or_init(|| {
        lib()["items"].as_array().map(|a| a.iter().enumerate().filter_map(|(i, x)| Some((x["id"].as_str()?.to_string(), i))).collect()).unwrap_or_default()
    })
}

fn lib_item(id: &str) -> Option<&'static Value> {
    index().get(id).and_then(|i| lib()["items"].get(*i))
}

/// An exercise of the library or one of your own.
pub fn by_id(id: &str) -> Option<Value> {
    lib_item(id).cloned().or_else(|| custom_snapshot().into_iter().find(|x| x["id"].as_str() == Some(id)))
}

/// The whole library for the UI (cached there): built-in items, then your own; `runnerOfp` = the
/// runner's ОФП list (1.4).
pub fn list() -> Value {
    let mut items = lib()["items"].as_array().cloned().unwrap_or_default();
    items.extend(custom_snapshot());
    json!({"source": lib()["source"], "version": lib()["version"], "items": items, "runnerOfp": extra()["runnerOfp"], "bandList": extra()["bandList"]})
}

// ---------- your complexes and exercises (1.4) ----------

/// Settings `complexes`: [{id, name, areas: [pain area keys], note, items: [{exId, leg}]}] and
/// `custom_exercises`: [{id "c_…", ru, tip, p, s, st, leg, bw}]. A complex keeps only its list
/// of exercises (repeats allowed); rounds, reps and load are entered on the workout page.
pub fn complexes(conn: &Connection) -> Value {
    let cx: Vec<Value> = db::get_setting(conn, "complexes").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    json!({"complexes": cx, "custom": custom_snapshot()})
}

fn clean_custom(n: usize, x: &Value) -> Option<Value> {
    let ru = x["ru"].as_str()?.trim();
    if ru.is_empty() {
        return None;
    }
    let id = x["id"].as_str().filter(|s| s.starts_with("c_")).map(String::from).unwrap_or_else(|| format!("c_{}_{n}", crate::store::now()));
    let strs = |v: &Value| -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|y| y.as_str().map(String::from)).collect()).unwrap_or_default() };
    Some(json!({
        "id": id, "ru": ru, "en": "", "syn": strs(&x["syn"]), "tip": x["tip"].as_str().unwrap_or("").trim(),
        "cat": "strength", "eq": x["eq"].as_str().unwrap_or("body only"), "lvl": "", "mech": "", "force": "",
        "p": strs(&x["p"]), "s": strs(&x["s"]), "bw": x["bw"].as_f64(), "st": x["st"].as_bool().unwrap_or(false),
        "leg": x["leg"].as_bool().unwrap_or(false), "img": 0, "own": true,
    }))
}

fn clean_complex(x: &Value, n: usize) -> Option<Value> {
    let name = x["name"].as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let items: Vec<Value> = x["items"]
        .as_array()
        .map(|a| a.iter().filter_map(|i| {
            let id = i["exId"].as_str().filter(|s| !s.is_empty())?;
            Some(json!({"exId": id, "leg": i["leg"].as_bool()}))
        }).collect())
        .unwrap_or_default();
    Some(json!({
        "id": x["id"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| format!("x_{}_{n}", crate::store::now())),
        "name": name, "note": x["note"].as_str().unwrap_or("").trim(),
        "areas": x["areas"].as_array().cloned().unwrap_or_default(), "items": items,
    }))
}

pub fn save_complexes(conn: &Connection, cx: &[Value], custom: &[Value]) -> Result<(), String> {
    let custom: Vec<Value> = custom.iter().enumerate().filter_map(|(n, x)| clean_custom(n, x)).collect();
    let cx: Vec<Value> = cx.iter().enumerate().filter_map(|(n, x)| clean_complex(x, n)).collect();
    db::set_setting(conn, "custom_exercises", &serde_json::to_string(&custom).unwrap_or_else(|_| "[]".into()))?;
    db::set_setting(conn, "complexes", &serde_json::to_string(&cx).unwrap_or_else(|_| "[]".into()))?;
    load_custom(conn);
    Ok(())
}

/// Adds complexes and exercises from a file ({complexes, custom}); ones already there (same id
/// or same name) are left as they are. Returns (complexes added, exercises added).
pub fn import_complexes(conn: &Connection, text: &str) -> Result<(usize, usize), String> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| "Это не файл комплексов Kouros (нужен JSON).".to_string())?;
    let cur = complexes(conn);
    let mut cx: Vec<Value> = cur["complexes"].as_array().cloned().unwrap_or_default();
    let mut custom: Vec<Value> = cur["custom"].as_array().cloned().unwrap_or_default();
    let (mut nc, mut ne) = (0, 0);
    // An exercise you already have under the same name keeps its id; the file's complexes point to it.
    let mut remap: HashMap<String, String> = HashMap::new();
    for x in v["custom"].as_array().into_iter().flatten() {
        if let Some(c) = custom.iter().find(|c| c["id"] == x["id"] || c["ru"] == x["ru"]) {
            if let (Some(a), Some(b)) = (x["id"].as_str(), c["id"].as_str()) {
                remap.insert(a.to_string(), b.to_string());
            }
        } else {
            custom.push(x.clone());
            ne += 1;
        }
    }
    for x in v["complexes"].as_array().into_iter().flatten() {
        if !cx.iter().any(|c| c["id"] == x["id"] || c["name"] == x["name"]) {
            let mut x = x.clone();
            if let Some(items) = x["items"].as_array_mut() {
                for i in items.iter_mut() {
                    let to: Option<String> = i["exId"].as_str().and_then(|id| remap.get(id)).cloned();
                    if let Some(to) = to {
                        i["exId"] = json!(to);
                    }
                }
            }
            cx.push(x);
            nc += 1;
        }
    }
    if nc == 0 && ne == 0 && v["complexes"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
        return Err("В файле нет комплексов.".into());
    }
    save_complexes(conn, &cx, &custom)?;
    Ok((nc, ne))
}

/// Your weight around a date: median of the weighings of the 30 days before it, else the latest one.
pub fn body_kg(conn: &Connection, date: &str) -> Option<f64> {
    let from = add_days(date, -30);
    let mut v: Vec<f64> = conn
        .prepare("SELECT num FROM hm WHERE metric = 'body_mass' AND num > 20 AND date BETWEEN ?1 AND ?2")
        .and_then(|mut st| st.query_map(params![from, date], |r| r.get::<_, f64>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    if v.is_empty() {
        return conn.query_row("SELECT num FROM hm WHERE metric = 'body_mass' AND num > 20 ORDER BY date DESC LIMIT 1", [], |r| r.get(0)).ok();
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

// ---------- resistance bands («Инвентарь») ----------

/// Bands are kept in settings `bands`: [{id, name, color, lo, hi, unit: "kg" | "lb"}].
pub fn bands(conn: &Connection) -> Vec<Value> {
    db::get_setting(conn, "bands").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_bands(conn: &Connection, v: &[Value]) -> Result<(), String> {
    let clean: Vec<Value> = v
        .iter()
        .filter_map(|b| {
            let lo = b["lo"].as_f64()?;
            let hi = b["hi"].as_f64().unwrap_or(lo);
            Some(json!({
                "id": b["id"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| format!("b{}", crate::store::now())),
                "name": b["name"].as_str().unwrap_or("").trim(), "color": b["color"].as_str().unwrap_or(""),
                "lo": lo.min(hi), "hi": hi.max(lo), "unit": if b["unit"] == "lb" { "lb" } else { "kg" },
            }))
        })
        .collect();
    db::set_setting(conn, "bands", &serde_json::to_string(&clean).unwrap_or_else(|_| "[]".into()))
}

/// A band's average pull over a repetition, kg: the middle of the range printed on it.
fn band_kg(b: &Value) -> f64 {
    let lo = b["lo"].as_f64().unwrap_or(0.0);
    let hi = b["hi"].as_f64().unwrap_or(lo);
    let k = if b["unit"] == "lb" { 0.4536 } else { 1.0 };
    (lo + hi) / 2.0 * k
}

/// Effective load of one repetition, kg, and its parts: (total, from body, from bands).
/// total = weight on the bar + share of body weight (own-weight exercises) ± bands
/// (`mode` "assist" — the band helps, e.g. pull-ups with a band — subtracts).
pub fn effective(set: &Value, body: Option<f64>, bands: &[Value]) -> (Option<f64>, Option<f64>, Option<f64>) {
    let ex = set["exId"].as_str().and_then(by_id);
    let ex = ex.as_ref();
    if ex.map(|e| e["st"].as_bool() == Some(true)).unwrap_or(false) {
        return (None, None, None); // static hold: time under load, not kilograms
    }
    let w = set["weightKg"].as_f64().filter(|w| *w >= 0.0);
    let bw = ex.and_then(|e| e["bw"].as_f64()).zip(body).map(|(k, b)| k * b);
    let ids: Vec<&str> = set["band"]["ids"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let bk: f64 = ids.iter().filter_map(|id| bands.iter().find(|b| b["id"].as_str() == Some(id))).map(band_kg).sum();
    let band = if ids.is_empty() { None } else if set["band"]["mode"] == "assist" { Some(-bk) } else { Some(bk) };
    if w.is_none() && bw.is_none() && band.is_none() {
        return (None, None, None);
    }
    let total = (w.unwrap_or(0.0) + bw.unwrap_or(0.0) + band.unwrap_or(0.0)).max(0.0);
    (Some((total * 10.0).round() / 10.0), bw.map(|x| (x * 10.0).round() / 10.0), band.map(|x| (x * 10.0).round() / 10.0))
}

// ---------- muscles ----------

/// Muscle groups (`muscle_svc::GROUPS` order) of an exercise: primary muscles count fully,
/// secondary half; leg-dominant work also loads the knees, calf work the ankles.
pub fn groups_in(id: &str, custom: &[Value]) -> Option<[f64; 12]> {
    let lib: Option<&Value> = lib_item(id);
    let ex = lib.or_else(|| custom.iter().find(|x| x["id"].as_str() == Some(id)))?;
    let mut g = [0.0f64; 12];
    let mut put = |m: &str, w: f64| match m {
        "calves" => { g[0] += w; g[11] += 0.15 * w }
        "quadriceps" => { g[1] += w; g[10] += 0.15 * w }
        "hamstrings" => g[2] += w,
        "glutes" => g[3] += w,
        "adductors" | "abductors" => g[4] += w,
        "abdominals" => g[5] += w,
        "lower back" => { g[6] += 0.6 * w; g[5] += 0.4 * w }
        "middle back" | "lats" | "traps" => g[6] += w,
        "chest" => g[7] += w,
        "shoulders" | "neck" => g[8] += w,
        "biceps" | "triceps" | "forearms" => g[9] += w,
        _ => {}
    };
    for m in ex["p"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
        put(m, 1.0);
    }
    for m in ex["s"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
        put(m, 0.5);
    }
    let s: f64 = g.iter().sum();
    if s <= 0.0 {
        return None;
    }
    for x in g.iter_mut() {
        *x /= s;
    }
    Some(g)
}

/// Date of a workout in the athlete's time zone.
pub fn activity_date(conn: &Connection, id: &str) -> Option<String> {
    let tz = crate::engine_svc::profile(conn).tz_offset_s;
    let t: i64 = conn.query_row("SELECT start_utc FROM activities WHERE id = ?1", params![id], |r| r.get(0)).ok()?;
    Some(crate::parse::time::ymd(t, tz))
}
