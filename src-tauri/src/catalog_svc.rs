//! Supplement catalog and medicine registry (0.18).
//!
//! - Built-in supplements by brand (Maxler — maxler.com, Europe; NOW and Solgar — NIH DSLD),
//!   collected at build time by `tools/catalog/build.py` into `resources/catalog.json.gz`.
//! - Online search in the NIH Dietary Supplement Label Database for anything else (only the typed
//!   text leaves the computer; settings `supp_online`, on by default).
//! - Medicines registered in Russia: trade name → МНН (built-in list from grls.rosminzdrav.ru,
//!   updated by a button into table `drug_registry`), group by МНН (`drugs.rs`), RxClass online for
//!   Latin names.
//! - Label text from Claude: lines «[ДОБАВКА] …» parsed into a preparation.

use crate::{db, drugs};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

static CATALOG_GZ: &[u8] = include_bytes!("../resources/catalog.json.gz");
static DRUGS_GZ: &[u8] = include_bytes!("../resources/drugs.json.gz");

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn gunzip(b: &[u8]) -> Value {
    let mut s = String::new();
    let mut d = flate2::read::GzDecoder::new(b);
    if d.read_to_string(&mut s).is_err() {
        return Value::Null;
    }
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

fn catalog() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| gunzip(CATALOG_GZ))
}

fn builtin_drugs() -> &'static Value {
    static D: OnceLock<Value> = OnceLock::new();
    D.get_or_init(|| gunzip(DRUGS_GZ))
}

fn lc(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

// ---------- nutrients: names → Kouros keys ----------

/// (key, unit, Russian name, matchers: "^x" = starts with, else contains).
pub const NUT: [(&str, &str, &str, &[&str]); 37] = [
    ("dietary_vitamin_a_total", "мкг", "Витамин A", &["^vitamin a", "retinol", "beta-carotene", "beta carotene", "^витамин a", "^витамин а", "ретинол", "бета-каротин"]),
    ("dietary_vitamin_c_total", "мг", "Витамин C", &["^vitamin c", "ascorb", "^витамин c", "^витамин с", "аскорбин"]),
    ("dietary_vitamin_d_total", "мкг", "Витамин D", &["^vitamin d", "cholecalciferol", "ergocalciferol", "^витамин d", "^витамин д", "холекальциферол", "^d3", "^д3"]),
    ("dietary_vitamin_e_total", "мг", "Витамин E", &["^vitamin e", "tocopher", "^витамин e", "^витамин е", "токоферол"]),
    ("dietary_vitamin_k_total", "мкг", "Витамин K", &["^vitamin k", "phylloquinone", "menaquinone", "^витамин k", "^витамин к", "менахинон", "филлохинон", "^k2", "^к2"]),
    ("dietary_vitamin_b12_total", "мкг", "B12", &["vitamin b12", "vitamin b-12", "cobalamin", "кобаламин", "витамин b12", "витамин в12"]),
    ("dietary_thiamin_total", "мг", "B1 (тиамин)", &["thiamin", "vitamin b1", "vitamin b-1", "тиамин", "витамин b1", "витамин в1"]),
    ("dietary_riboflavin_total", "мг", "B2 (рибофлавин)", &["riboflavin", "vitamin b2", "vitamin b-2", "рибофлавин", "витамин b2", "витамин в2"]),
    ("dietary_niacin_total", "мг", "B3 (ниацин)", &["niacin", "nicotinamide", "vitamin b3", "ниацин", "никотинамид", "витамин b3", "витамин в3", "витамин pp", "витамин рр"]),
    ("dietary_vitamin_b6_total", "мг", "B6", &["vitamin b6", "vitamin b-6", "pyridox", "пиридокс", "витамин b6", "витамин в6"]),
    ("dietary_folate_total", "мкг", "B9 (фолат)", &["folate", "folic", "methylfolate", "фолат", "фолиев", "метилфолат", "витамин b9", "витамин в9"]),
    ("dietary_biotin_total", "мкг", "Биотин", &["biotin", "биотин", "витамин b7", "витамин н"]),
    ("dietary_pantothenic_acid_total", "мг", "B5 (пантотеновая к-та)", &["pantothen", "vitamin b5", "vitamin b-5", "пантотен", "витамин b5", "витамин в5"]),
    ("dietary_calcium_total", "мг", "Кальций", &["^calcium", "^кальци"]),
    ("dietary_iron_total", "мг", "Железо", &["^iron", "^железо", "^желез", "бисглицинат железа", "фумарат железа"]),
    ("dietary_magnesium_total", "мг", "Магний", &["^magnesium", "^магни", "цитрат магния", "глицинат магния", "бисглицинат магния", "оксид магния"]),
    ("dietary_zinc_total", "мг", "Цинк", &["^zinc", "^цинк", "пиколинат цинка", "цитрат цинка", "глюконат цинка"]),
    ("dietary_potassium_total", "мг", "Калий", &["^potassium", "^кали"]),
    ("dietary_selenium_total", "мкг", "Селен", &["^selenium", "^селен", "селенометионин"]),
    ("dietary_phosphorus_total", "мг", "Фосфор", &["^phosphorus", "^фосфор"]),
    ("dietary_copper_total", "мг", "Медь", &["^copper", "^медь", "^меди"]),
    ("dietary_manganese_total", "мг", "Марганец", &["^manganese", "^марган"]),
    ("dietary_sodium_total", "мг", "Натрий", &["^sodium", "^натри"]),
    ("dietary_iodine_total", "мкг", "Йод", &["^iodine", "^йод", "иодид"]),
    ("dietary_chromium_total", "мкг", "Хром", &["^chromium", "^хром", "пиколинат хрома"]),
    ("choline", "мг", "Холин", &["^choline", "^холин"]),
    ("dietary_caffeine_total", "мг", "Кофеин", &["caffeine", "кофеин"]),
    ("dietary_protein_total", "г", "Белок", &["^protein", "^белок", "^белки", "^протеин"]),
    ("dietary_carbohydrates_total", "г", "Углеводы", &["carbohydrate", "углевод"]),
    ("dietary_fat_total_total", "г", "Жиры", &["^total fat", "^fat", "^жир"]),
    ("dietary_energy_total", "ккал", "Энергия", &["^calories", "^energy", "^энергет", "^калори", "^ккал"]),
    ("dietary_fiber_total", "г", "Клетчатка", &["^fiber", "^fibre", "^клетчатк", "пищевые волокна"]),
    ("epa", "мг", "Омега-3 EPA", &["^epa", "eicosapentaeno", "эйкозапентаен", "эпк"]),
    ("dha", "мг", "Омега-3 DHA", &["^dha", "docosahexaeno", "докозагексаен", "дгк"]),
    ("creatine", "мг", "Креатин", &["^creatine", "^креатин"]),
    ("melatonin", "мг", "Мелатонин", &["melatonin", "мелатонин"]),
    ("water_ml", "мл", "Вода", &["^water", "^вода"]),
];

/// The Kouros key and unit for a component name (Russian or English).
pub fn nut_key(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let n = lc(name.trim());
    for (k, u, ru, pats) in NUT.iter() {
        for p in pats.iter() {
            let hit = match p.strip_prefix('^') {
                Some(pre) => n.starts_with(pre),
                None => n.contains(p),
            };
            if hit {
                return Some((k, u, ru));
            }
        }
    }
    None
}

/// Amount in the Kouros unit (None if the units can't be converted).
pub fn convert(key: &str, target: &str, amount: f64, unit: &str) -> Option<f64> {
    let u = lc(unit.trim()).replace(['µ', 'μ'], "mc");
    if u == "iu" || u == "ме" || u == "ie" {
        return match key {
            "dietary_vitamin_d_total" => Some(amount / 40.0),
            "dietary_vitamin_a_total" => Some(amount * 0.3),
            "dietary_vitamin_e_total" => Some(amount * 0.67),
            _ => None,
        };
    }
    let mass = |u: &str| -> Option<f64> {
        if u.starts_with("mcg") || u.starts_with("мкг") || u == "ug" {
            Some(0.001)
        } else if u.starts_with("mg") || u.starts_with("мг") {
            Some(1.0)
        } else if u == "g" || u == "г" || u.starts_with("gram") {
            Some(1000.0)
        } else {
            None
        }
    };
    match target {
        "мг" | "мкг" | "г" => {
            let t = match target {
                "мг" => 1.0,
                "мкг" => 0.001,
                _ => 1000.0,
            };
            mass(&u).map(|f| amount * f / t)
        }
        "ккал" => Some(if u.contains("kj") || u.contains("кдж") { amount / 4.184 } else { amount }),
        _ => Some(amount),
    }
}

/// One component as stored in a preparation: {key, name, amount, unit}.
pub fn component(name: &str, amount: f64, unit: &str) -> Value {
    if let Some((k, u, ru)) = nut_key(name) {
        if let Some(a) = convert(k, u, amount, unit) {
            return json!({"key": k, "name": ru, "label": name, "amount": (a * 10000.0).round() / 10000.0, "unit": u});
        }
    }
    json!({"key": "", "name": name, "amount": amount, "unit": unit})
}

// ---------- built-in catalog ----------

pub fn brands() -> Value {
    let c = catalog();
    let prods = c["products"].as_array().cloned().unwrap_or_default();
    let list: Vec<Value> = c["brands"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|b| {
            let key = b["key"].as_str().unwrap_or("").to_string();
            let mine: Vec<&Value> = prods.iter().filter(|p| p["b"].as_str() == Some(key.as_str())).collect();
            let count = |t: &str| mine.iter().filter(|p| p["t"].as_str() == Some(t)).count();
            json!({"key": key, "name": b["name"], "count": mine.len(),
                   "kinds": {"vitamin": count("vitamin"), "protein": count("protein"), "isotonic": count("isotonic"), "amino": count("amino"), "other": count("other")}})
        })
        .collect();
    json!({"version": c["version"], "brands": list})
}

fn brief(p: &Value) -> Value {
    let comps = p["c"].as_array().cloned().unwrap_or_default();
    let names: Vec<String> = comps.iter().take(4).filter_map(|c| c[1].as_str().map(String::from)).collect();
    json!({"id": p["id"], "brand": p["b"], "name": p["n"], "serving": p["s"], "form": p["f"], "kind": p["t"], "preview": names.join(", "), "count": comps.len()})
}

/// Search in the built-in catalog (brand and kind optional; all words must match).
pub fn search(brand: Option<&str>, kind: Option<&str>, q: &str, limit: usize) -> Vec<Value> {
    let words: Vec<String> = lc(q).split_whitespace().map(String::from).collect();
    let prods = catalog()["products"].as_array().cloned().unwrap_or_default();
    let mut out: Vec<Value> = prods
        .iter()
        .filter(|p| brand.map(|b| p["b"].as_str() == Some(b)).unwrap_or(true))
        .filter(|p| kind.map(|k| p["t"].as_str() == Some(k)).unwrap_or(true))
        .filter(|p| {
            let n = lc(&format!("{} {}", p["n"].as_str().unwrap_or(""), p["b"].as_str().unwrap_or("")));
            words.iter().all(|w| n.contains(w.as_str()))
        })
        .map(brief)
        .collect();
    out.sort_by(|a, b| a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")));
    out.truncate(limit);
    out
}

/// Water for an isotonic: «… with 500 ml of water» in the serving text or name.
fn water_of(text: &str) -> Option<f64> {
    let l = lc(text);
    let i = l.find("ml").or_else(|| l.find("мл"))?;
    let head: String = l[..i].chars().rev().skip_while(|c| *c == ' ').take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    head.parse::<f64>().ok().filter(|v| *v >= 100.0 && *v <= 2000.0)
}

/// A catalog product as a preparation ready for the editor.
pub fn product(id: &str) -> Option<Value> {
    let p = catalog()["products"].as_array()?.iter().find(|p| p["id"].as_str() == Some(id))?.clone();
    let comps: Vec<Value> = p["c"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|c| {
            let key = c[0].as_str().unwrap_or("");
            let ru = NUT.iter().find(|n| n.0 == key).map(|n| n.2).unwrap_or("");
            let name = if key.is_empty() || ru.is_empty() { c[1].as_str().unwrap_or("") } else { ru };
            json!({"key": key, "name": name, "label": c[1], "amount": c[2], "unit": c[3]})
        })
        .collect();
    let kind = p["t"].as_str().unwrap_or("other");
    let brand = catalog()["brands"].as_array().and_then(|b| b.iter().find(|x| x["key"] == p["b"]).and_then(|x| x["name"].as_str().map(String::from))).unwrap_or_default();
    let schedule = if kind == "isotonic" || kind == "protein" { "asneeded" } else { "daily" };
    let water = if kind == "isotonic" { water_of(p["s"].as_str().unwrap_or("")).or(Some(500.0)) } else { None };
    let full = format!("{} {}", brand, p["n"].as_str().unwrap_or("")).trim().to_string();
    let form = form_ru(p["f"].as_str().unwrap_or(""), p["s"].as_str().unwrap_or(""));
    let count_food = kind == "isotonic";
    Some(json!({
        "kind": "supplement", "name": full, "form": form,
        "dose": 1, "schedule": schedule, "components": comps,
        "extra": {"source": id, "brand": p["b"], "serving": p["s"], "catalogKind": kind, "countFood": count_food, "waterMl": water},
    }))
}

fn form_ru(f: &str, serving: &str) -> String {
    let t = lc(&format!("{f} {serving}"));
    if t.contains("softgel") {
        "капсула".into()
    } else if t.contains("capsule") || t.contains("капсул") {
        "капсула".into()
    } else if t.contains("tablet") || t.contains("таблет") {
        "таблетка".into()
    } else if t.contains("scoop") || t.contains("powder") || t.contains("порош") {
        "порция порошка".into()
    } else if t.contains("drop") || t.contains("liquid") || t.contains("капл") {
        "капли".into()
    } else if t.contains("gumm") {
        "мармеладка".into()
    } else {
        "порция".into()
    }
}

// ---------- NIH DSLD online ----------

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(60)).user_agent("Kouros").build().map_err(err)
}

pub fn online_enabled(conn: &Connection) -> bool {
    db::get_setting(conn, "supp_online").as_deref() != Some("0")
}

fn dsld_label(s: &Value) -> Value {
    let comps: Vec<Value> = s["ingredientRows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            let q = r["quantity"].as_array()?.first()?.clone();
            let a = q["quantity"].as_f64().filter(|a| *a > 0.0)?;
            let name = r["ingredientGroup"].as_str().filter(|g| nut_key(g).is_some()).or(r["name"].as_str())?;
            Some(component(name, a, q["unit"].as_str().unwrap_or("")))
        })
        .collect();
    let ss = s["servingSizes"].as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
    let serving = format!("{} {}", ss["minQuantity"].as_f64().unwrap_or(1.0), ss["unit"].as_str().unwrap_or("")).trim().to_string();
    json!({
        "id": format!("dsld:{}", s["id"]), "brand": s["brandName"], "name": s["fullName"], "serving": serving, "offMarket": s["offMarket"],
        "prep": {"kind": "supplement", "name": format!("{} {}", s["brandName"].as_str().unwrap_or(""), s["fullName"].as_str().unwrap_or("")).trim(),
                 "form": form_ru(s["physicalState"]["langualCodeDescription"].as_str().unwrap_or(""), &serving), "dose": 1, "schedule": "daily", "components": comps,
                 "extra": {"source": format!("dsld:{}", s["id"]), "serving": serving, "countFood": false}},
    })
}

pub async fn search_online(q: &str) -> Result<Vec<Value>, String> {
    let url = format!("https://api.ods.od.nih.gov/dsld/v9/search-filter?q={}&size=25", urlenc(q));
    let v: Value = http()?.get(url).send().await.map_err(|e| format!("База NIH недоступна: {e}"))?.json().await.map_err(err)?;
    let hits = v["hits"].as_array().cloned().unwrap_or_default();
    let mut out: Vec<Value> = hits.iter().map(|h| dsld_label(&h["_source"])).filter(|x| x["prep"]["components"].as_array().map(|a| !a.is_empty()).unwrap_or(false)).collect();
    // On the market first.
    out.sort_by_key(|x| x["offMarket"].as_i64().unwrap_or(0));
    Ok(out)
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---------- label text from Claude ----------

/// «[ДОБАВКА] Название | порция: 1 капсула | Витамин D3 50 мкг; Магний 200 мг» → a preparation.
pub fn from_label(text: &str) -> Option<Value> {
    let line = text.lines().map(str::trim).find(|l| l.to_uppercase().starts_with("[ДОБАВКА]"))?;
    let rest = line[line.find(']')? + 1..].trim();
    let parts: Vec<&str> = rest.split('|').map(str::trim).collect();
    let name = parts.first()?.to_string();
    let mut serving = String::new();
    let mut comps = Vec::new();
    for p in parts.iter().skip(1) {
        let l = lc(p);
        if l.starts_with("порция") || l.starts_with("serving") {
            serving = p.split_once(':').map(|x| x.1.trim().to_string()).unwrap_or_default();
            continue;
        }
        for c in p.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            // «Витамин D3 50 мкг» — the last number and unit.
            let toks: Vec<&str> = c.split_whitespace().collect();
            let pos = toks.iter().rposition(|t| t.replace(',', ".").parse::<f64>().is_ok());
            if let Some(i) = pos {
                let amount: f64 = toks[i].replace(',', ".").parse().unwrap_or(0.0);
                let unit = toks.get(i + 1).copied().unwrap_or("");
                let nm = toks[..i].join(" ");
                if !nm.is_empty() && amount > 0.0 {
                    comps.push(component(&nm, amount, unit));
                }
            }
        }
    }
    if comps.is_empty() {
        return None;
    }
    Some(json!({"kind": "supplement", "name": name, "form": form_ru("", &serving), "dose": 1, "schedule": "daily", "components": comps,
                "extra": {"source": "label", "serving": serving, "countFood": false}}))
}

// ---------- medicines ----------

fn registry_rows(conn: &Connection) -> Vec<(String, String, String, String)> {
    let local: Vec<(String, String, String, String)> = conn
        .prepare("SELECT trade, inn, form, dose FROM drug_registry")
        .ok()
        .map(|mut st| {
            let v: Vec<(String, String, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()).unwrap_or_default();
            v
        })
        .unwrap_or_default();
    if !local.is_empty() {
        return local;
    }
    builtin_drugs()["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|r| Some((r[0].as_str()?.to_string(), r[1].as_str()?.to_string(), r[2].as_str().unwrap_or("").to_string(), r[3].as_str().unwrap_or("").to_string())))
        .collect()
}

pub fn registry_info(conn: &Connection) -> Value {
    let n: i64 = conn.query_row("SELECT count(*) FROM drug_registry", [], |r| r.get(0)).unwrap_or(0);
    json!({
        "builtin": builtin_drugs()["rows"].as_array().map(|a| a.len()).unwrap_or(0), "builtinVersion": builtin_drugs()["version"], "builtinSource": builtin_drugs()["source"],
        "local": n, "localVersion": db::get_setting(conn, "drug_registry_version"),
    })
}

fn group_json(inn: &str) -> Value {
    match drugs::group_of_inn(inn).and_then(drugs::group) {
        Some((k, n, t)) => json!({"key": k, "name": n, "note": t}),
        None => Value::Null,
    }
}

/// Medicines by trade name or МНН (registry + built-in names).
pub fn drug_search(conn: &Connection, q: &str) -> Vec<Value> {
    let ql = lc(q.trim());
    if ql.chars().count() < 2 {
        return Vec::new();
    }
    let mut out: Vec<Value> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (t, inn, form, dose) in registry_rows(conn) {
        let (tl, il) = (lc(&t), lc(&inn));
        if !(tl.starts_with(&ql) || il.starts_with(&ql) || tl.contains(&format!(" {ql}"))) {
            continue;
        }
        let key = (tl.clone(), il.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(json!({"trade": t, "inn": inn, "form": form, "dose": dose, "group": group_json(&inn), "source": "registry"}));
        if out.len() >= 30 {
            break;
        }
    }
    if out.is_empty() {
        if let Some(inn) = drugs::inn_of_trade(&ql) {
            out.push(json!({"trade": q.trim(), "inn": inn, "form": "", "dose": "", "group": group_json(inn), "source": "builtin"}));
        } else if drugs::group_of_inn(&ql).is_some() {
            out.push(json!({"trade": q.trim(), "inn": q.trim(), "form": "", "dose": "", "group": group_json(&ql), "source": "builtin"}));
        }
    }
    out
}

/// RxClass (NIH) by a Latin active ingredient → our group.
pub async fn drug_online(inn_latin: &str) -> Result<Value, String> {
    let url = format!("https://rxnav.nlm.nih.gov/REST/rxclass/class/byDrugName.json?drugName={}&relaSource=ATC", urlenc(inn_latin));
    let v: Value = http()?.get(url).send().await.map_err(|e| format!("База RxNav недоступна: {e}"))?.json().await.map_err(err)?;
    let classes: Vec<String> = v["rxclassDrugInfoList"]["rxclassDrugInfo"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["rxclassMinConceptItem"]["classId"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    let key = classes.iter().find_map(|c| {
        let g = if c.starts_with("M01A") {
            "nsaid"
        } else if c.starts_with("J01MA") {
            "fluoroquinolone"
        } else if c.starts_with("C07") {
            "beta_blocker"
        } else if c.starts_with("R06A") {
            "antihistamine"
        } else if c.starts_with("N05C") {
            "hypnotic"
        } else if c.starts_with("H02") {
            "corticosteroid"
        } else if c.starts_with("C10AA") {
            "statin"
        } else if c.starts_with("C03") {
            "diuretic"
        } else {
            return None;
        };
        Some(g)
    });
    Ok(json!({"classes": classes, "group": key.and_then(drugs::group).map(|(k, n, t)| json!({"key": k, "name": n, "note": t}))}))
}

/// «Обновить реестр»: the latest list from grls.rosminzdrav.ru into table `drug_registry`.
pub async fn registry_download() -> Result<(String, Vec<(String, String, String, String)>), String> {
    let c = http()?;
    let page = c.get("https://grls.rosminzdrav.ru/Default.aspx").send().await.map_err(|e| format!("Сайт реестра недоступен: {e}"))?.text().await.map_err(err)?;
    let link = page
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .find(|h| (h.ends_with(".xls") || h.ends_with(".xlsx")) && h.contains("еречень"))
        .ok_or("На сайте реестра не нашлась ссылка на перечень")?
        .replace("http://", "https://");
    let url: String = link.chars().map(|ch| if ch.is_ascii() && ch != ' ' { ch.to_string() } else { ch.to_string().bytes().map(|b| format!("%{b:02X}")).collect() }).collect();
    let bytes = c.get(&url).send().await.map_err(err)?.bytes().await.map_err(err)?;
    let name = link.rsplit('/').next().unwrap_or("").to_string();
    let rows = parse_xls(&bytes)?;
    Ok((name, rows))
}

fn parse_xls(bytes: &[u8]) -> Result<Vec<(String, String, String, String)>, String> {
    use calamine::Reader;
    let mut wb = calamine::Xls::new(std::io::Cursor::new(bytes.to_vec())).map_err(|e| format!("Не удалось прочитать файл реестра: {e}"))?;
    let mut out = Vec::new();
    let names = wb.sheet_names().to_vec();
    for n in names {
        let Ok(range) = wb.worksheet_range(&n) else { continue };
        let mut head: Option<(usize, usize, Option<usize>, Option<usize>)> = None;
        let mut inn = String::new();
        for row in range.rows() {
            let vals: Vec<String> = row.iter().map(|c| c.to_string().trim().to_string()).collect();
            if head.is_none() {
                if let Some(t) = vals.iter().position(|v| v.contains("Торговое")) {
                    let i = vals.iter().position(|v| v.starts_with("МНН")).unwrap_or(0);
                    let f = vals.iter().position(|v| v.to_lowercase().contains("форма"));
                    let d = vals.iter().position(|v| v.contains("Дозировка"));
                    head = Some((i, t, f, d));
                }
                continue;
            }
            let (i, t, f, d) = head.unwrap();
            if let Some(v) = vals.get(i).filter(|v| !v.is_empty()) {
                inn = v.trim_start_matches('*').trim().to_string();
            }
            let trade = vals.get(t).map(|v| v.replace(['®', '™'], "").trim().to_string()).unwrap_or_default();
            if trade.is_empty() || inn.is_empty() {
                continue;
            }
            out.push((trade, inn.clone(), f.and_then(|f| vals.get(f).cloned()).unwrap_or_default(), d.and_then(|d| vals.get(d).cloned()).unwrap_or_default()));
        }
    }
    if out.is_empty() {
        return Err("В файле реестра не нашлись лекарства".into());
    }
    Ok(out)
}

pub fn registry_store(conn: &mut Connection, name: &str, rows: &[(String, String, String, String)]) -> Result<usize, String> {
    let tx = conn.transaction().map_err(err)?;
    tx.execute("DELETE FROM drug_registry", []).map_err(err)?;
    {
        let mut st = tx.prepare("INSERT INTO drug_registry (trade, inn, form, dose) VALUES (?1, ?2, ?3, ?4)").map_err(err)?;
        for r in rows {
            st.execute(params![r.0, r.1, r.2, r.3]).map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    db::set_setting(conn, "drug_registry_version", &format!("{} · {}", name, chrono::Local::now().format("%d.%m.%Y")))?;
    Ok(rows.len())
}
