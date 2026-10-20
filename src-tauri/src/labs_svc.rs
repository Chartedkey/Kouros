//! «Анализы» (0.20): lab reports from a PDF (read on this computer by `parse::pdf` + `parse::labs`)
//! or from Claude's «[АНАЛИЗ] …» lines for scans and photos. Results are kept with the lab's own
//! range; known tests get one key across labs (synonyms, unit conversion) and plain-word notes on
//! what the value means for training and food. Never a diagnosis — «обрати внимание / обсуди с врачом».

use crate::engine::add_days;
use crate::parse::labs::{parse_ref, parse_value, Report, Row};
use crate::store;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn lc(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

/// key, name, group, canonical unit, matchers (lowercase; "^" = starts with, "=" = whole name).
pub const TESTS: &[(&str, &str, &str, &str, &[&str])] = &[
    ("hgb", "Гемоглобин", "Кровь", "г/л", &["^гемоглобин", "=hgb", "^hgb", "=hb"]),
    ("rbc", "Эритроциты", "Кровь", "10*12/л", &["^эритроциты", "=rbc"]),
    ("hct", "Гематокрит", "Кровь", "%", &["^гематокрит", "=hct"]),
    ("mcv", "Средний объём эритроцитов (MCV)", "Кровь", "фл", &["(mcv)", "=mcv"]),
    ("wbc", "Лейкоциты", "Кровь", "10*9/л", &["^лейкоциты", "=wbc"]),
    ("plt", "Тромбоциты", "Кровь", "10*9/л", &["^тромбоциты", "=plt"]),
    ("esr", "СОЭ", "Кровь", "мм/час", &["^соэ", "скорость оседания"]),
    ("ferritin", "Ферритин", "Железо", "мкг/л", &["^ферритин"]),
    ("iron", "Железо в сыворотке", "Железо", "мкмоль/л", &["=железо", "^железо сыворот", "^железо в сыворот", "^железо (fe)"]),
    ("tsat", "Насыщение трансферрина железом", "Железо", "%", &["насыщения трансферрина", "насыщение трансферрина", "нтж"]),
    ("transferrin", "Трансферрин", "Железо", "г/л", &["^трансферрин"]),
    ("vitd", "Витамин D (25-OH)", "Витамины", "нг/мл", &["25-oh", "25(oh)", "25-гидрокси", "^витамин d", "кальциферол"]),
    ("b12", "Витамин B12", "Витамины", "пг/мл", &["b12", "в12", "цианокобаламин", "кобаламин"]),
    ("folate", "Фолиевая кислота (B9)", "Витамины", "нг/мл", &["фолиев", "фолат", "b9", "в9"]),
    ("vita", "Витамин A", "Витамины", "мкг/мл", &["^витамин а", "^витамин a", "ретинол"]),
    ("vite", "Витамин E", "Витамины", "мкг/мл", &["^витамин e", "^витамин е", "токоферол"]),
    ("vitc", "Витамин C", "Витамины", "мкг/мл", &["^витамин с", "^витамин c", "аскорбин"]),
    ("vitk", "Витамин K", "Витамины", "нг/мл", &["^витамин к", "^витамин k", "филлохинон"]),
    ("b1", "Витамин B1", "Витамины", "нмоль/л", &["тиамин", "^витамин в1", "^витамин b1"]),
    ("b6", "Витамин B6", "Витамины", "нмоль/л", &["пиридокс", "^витамин b6", "^витамин в6"]),
    ("magnesium", "Магний", "Минералы", "ммоль/л", &["^магний"]),
    ("zinc", "Цинк", "Минералы", "мкмоль/л", &["^цинк"]),
    ("calcium", "Кальций общий", "Минералы", "ммоль/л", &["^кальций общий", "=кальций"]),
    ("calcium_ion", "Кальций ионизированный", "Минералы", "ммоль/л", &["^кальций ионизир"]),
    ("potassium", "Калий", "Минералы", "ммоль/л", &["^калий"]),
    ("sodium", "Натрий", "Минералы", "ммоль/л", &["^натрий (", "=натрий"]),
    ("copper", "Медь", "Минералы", "мкмоль/л", &["^медь"]),
    ("phosphorus", "Фосфор", "Минералы", "ммоль/л", &["^фосфор"]),
    ("ck", "Креатинкиназа (КФК)", "Мышцы и печень", "Ед/л", &["креатинкиназа", "кфк", "=ck", "=cpk"]),
    ("alt", "АЛТ", "Мышцы и печень", "Ед/л", &["(алт)", "=алт", "аланинаминотрансфераза", "=alt"]),
    ("ast", "АСТ", "Мышцы и печень", "Ед/л", &["(аст)", "=аст", "аспартатаминотрансфераза", "=ast"]),
    ("ggt", "ГГТ", "Мышцы и печень", "Ед/л", &["(ггт)", "гамма-глутамил", "=ггт"]),
    ("ldh", "ЛДГ", "Мышцы и печень", "Ед/л", &["лактатдегидрогеназа", "(лдг)", "=лдг"]),
    ("creatinine", "Креатинин", "Почки", "мкмоль/л", &["^креатинин"]),
    ("urea", "Мочевина", "Почки", "ммоль/л", &["^мочевина"]),
    ("uric", "Мочевая кислота", "Почки", "мкмоль/л", &["^мочевая кислота"]),
    ("crp", "С-реактивный белок", "Воспаление", "мг/л", &["с-реактивный", "c-реактивный", "=срб", "=crp", "hs-crp"]),
    ("glucose", "Глюкоза", "Сахар", "ммоль/л", &["^глюкоза"]),
    ("hba1c", "Гликированный гемоглобин", "Сахар", "%", &["гликированный", "hba1c", "a1c"]),
    ("chol", "Холестерин общий", "Липиды", "ммоль/л", &["^холестерин общий", "=холестерин"]),
    ("nonhdl", "Холестерин не-ЛПВП", "Липиды", "ммоль/л", &["не-лпвп", "не лпвп", "non-hdl"]),
    ("hdl", "ЛПВП («хороший» холестерин)", "Липиды", "ммоль/л", &["(лпвп", "лпвп,", "высокой плотности", "=hdl"]),
    ("ldl", "ЛПНП («плохой» холестерин)", "Липиды", "ммоль/л", &["(лпнп", "лпнп,", "низкой плотности", "=ldl"]),
    ("tg", "Триглицериды", "Липиды", "ммоль/л", &["^триглицериды"]),
    ("tsh", "ТТГ", "Гормоны", "мкМЕ/мл", &["(ттг)", "=ттг", "тиреотропный", "=tsh"]),
    ("ft4", "Т4 свободный", "Гормоны", "пмоль/л", &["т4 свободный", "тироксин свободный", "free t4"]),
    ("ft3", "Т3 свободный", "Гормоны", "пмоль/л", &["т3 свободный", "трийодтиронин свободный", "free t3"]),
    ("testo", "Тестостерон общий", "Гормоны", "нмоль/л", &["^тестостерон общий", "=тестостерон"]),
    ("testo_free", "Тестостерон свободный", "Гормоны", "нмоль/л", &["^тестостерон свободный"]),
    ("shbg", "ГСПГ (SHBG)", "Гормоны", "нмоль/л", &["гспг", "shbg"]),
    ("cortisol", "Кортизол", "Гормоны", "нмоль/л", &["^кортизол"]),
    ("pth", "Паратгормон", "Гормоны", "пг/мл", &["^паратгормон", "паратиреоидный"]),
    ("omega3", "Омега-3 индекс", "Жирные кислоты", "%", &["омега-3 индекс для цельной", "=омега-3 индекс"]),
    ("homocysteine", "Гомоцистеин", "Прочее", "мкмоль/л", &["^гомоцистеин"]),
];

/// Known test key of a printed name (None = keep it under its own name).
pub fn key_of(name: &str) -> Option<&'static str> {
    let n = lc(name.trim());
    for (k, _, _, _, pats) in TESTS {
        for p in pats.iter() {
            let hit = if let Some(x) = p.strip_prefix('^') {
                n.starts_with(x)
            } else if let Some(x) = p.strip_prefix('=') {
                n == x
            } else {
                n.contains(p)
            };
            if hit {
                return Some(k);
            }
        }
    }
    None
}

fn test(key: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str, &'static [&'static str])> {
    TESTS.iter().find(|t| t.0 == key)
}

/// Value into the canonical unit of the test (unknown units stay as they are).
pub fn canon(key: &str, v: f64, unit: &str) -> (f64, String) {
    let u = lc(unit.trim()).replace(' ', "");
    let Some(t) = test(key) else { return (v, unit.to_string()) };
    let cu = t.3.to_string();
    let f = match (key, u.as_str()) {
        ("hgb", "г/дл") | ("hgb", "g/dl") => 10.0,
        ("ferritin", "нг/мл") | ("ferritin", "ng/ml") => 1.0,
        ("vitd", "нмоль/л") | ("vitd", "nmol/l") => 1.0 / 2.496,
        ("b12", "пмоль/л") | ("b12", "pmol/l") => 1.355,
        ("folate", "нмоль/л") | ("folate", "nmol/l") => 1.0 / 2.266,
        ("glucose", "мг/дл") | ("glucose", "mg/dl") => 1.0 / 18.0,
        ("chol" | "ldl" | "hdl" | "nonhdl", "мг/дл") | ("chol" | "ldl" | "hdl" | "nonhdl", "mg/dl") => 1.0 / 38.67,
        ("tg", "мг/дл") | ("tg", "mg/dl") => 1.0 / 88.57,
        ("creatinine", "мг/дл") | ("creatinine", "mg/dl") => 88.4,
        ("testo", "нг/мл") | ("testo", "ng/ml") => 3.467,
        ("testo", "нг/дл") | ("testo", "ng/dl") => 0.03467,
        ("cortisol", "мкг/дл") | ("cortisol", "µg/dl") | ("cortisol", "mcg/dl") => 27.59,
        _ => return (v, if u.is_empty() { cu } else { unit.to_string() }),
    };
    (v * f, cu)
}

// ---------- import ----------

fn row_json(r: &Row) -> Value {
    let key = key_of(&r.name);
    let name = key.and_then(test).map(|t| t.1).unwrap_or("");
    json!({"name": r.name, "valueText": r.value_text, "value": r.value, "cmp": r.cmp, "flag": r.flag, "unit": r.unit,
           "refText": r.ref_text, "lo": r.lo, "hi": r.hi, "section": r.section, "comment": r.comment.join("\n"),
           "key": key, "keyName": name})
}

/// Reads a PDF into a preview (nothing is saved yet).
pub fn parse_pdf(path: &str) -> Result<Value, String> {
    let data = std::fs::read(path).map_err(|e| format!("Не удалось открыть файл: {e}"))?;
    let runs = crate::parse::pdf::text_runs(&data)?;
    if runs.iter().filter(|r| r.text.chars().any(|c| c.is_alphabetic())).count() < 10 {
        return Err("В этом PDF нет текста — похоже, это скан или фото. Отправь его Claude и вставь ответ со строками [АНАЛИЗ].".into());
    }
    let rep = crate::parse::labs::read(&runs);
    if rep.rows.is_empty() {
        return Err("Не нашлось строк с результатами. Если бланк необычный — отправь его Claude и вставь ответ со строками [АНАЛИЗ].".into());
    }
    Ok(preview(&rep, Some(path), "pdf"))
}

fn preview(rep: &Report, path: Option<&str>, source: &str) -> Value {
    json!({"date": rep.date, "lab": rep.lab, "file": path, "source": source, "rows": rep.rows.iter().map(row_json).collect::<Vec<_>>()})
}

/// Claude's answer: «[АНАЛИЗ] Ферритин | 45 | нг/мл | 30–400 | 14.10.2026» (one line per result;
/// the date may be given once, «[АНАЛИЗЫ] 14.10.2026, KDL»).
pub fn parse_text(text: &str) -> Result<Value, String> {
    let mut rep = Report::default();
    for line in text.lines().map(str::trim) {
        let up = line.to_uppercase();
        if up.starts_with("[АНАЛИЗЫ]") {
            let rest = line[line.find(']').map(|i| i + 1).unwrap_or(0)..].trim();
            if let Some(d) = ru_date(rest) {
                rep.date = Some(d);
            }
            if let Some((_, lab)) = rest.split_once(',') {
                rep.lab = lab.trim().to_string();
            }
            continue;
        }
        if !up.starts_with("[АНАЛИЗ]") {
            continue;
        }
        let rest = line[line.find(']').map(|i| i + 1).unwrap_or(0)..].trim();
        let p: Vec<&str> = rest.split('|').map(str::trim).collect();
        if p.len() < 2 || p[0].is_empty() {
            continue;
        }
        let (vt, value, cmp, flag) = parse_value(p[1]);
        let unit = p.get(2).copied().unwrap_or("").to_string();
        let ref_text = p.get(3).copied().unwrap_or("").to_string();
        let (lo, hi) = parse_ref(&ref_text);
        if let Some(d) = p.get(4).and_then(|d| ru_date(d)) {
            rep.date.get_or_insert(d);
        }
        rep.rows.push(Row { name: p[0].to_string(), value_text: vt, value, cmp, flag, unit, ref_text, lo, hi, ..Default::default() });
    }
    if rep.rows.is_empty() {
        return Err("Не нашлось строк «[АНАЛИЗ] название | результат | единицы | норма».".into());
    }
    Ok(preview(&rep, None, "claude"))
}

fn ru_date(t: &str) -> Option<String> {
    let t = t.trim();
    for w in t.split(|c: char| c.is_whitespace() || c == ',') {
        let p: Vec<&str> = w.split('.').collect();
        if p.len() == 3 && p[0].len() <= 2 && p[1].len() <= 2 && p[2].len() == 4 && p.iter().all(|x| x.chars().all(|c| c.is_ascii_digit())) {
            return Some(format!("{}-{:0>2}-{:0>2}", p[2], p[1], p[0]));
        }
        if w.len() == 10 && w.as_bytes()[4] == b'-' && w.as_bytes()[7] == b'-' {
            return Some(w.to_string());
        }
    }
    None
}

/// Saves an (edited) preview; the PDF is copied next to the database (`files/labs`).
pub fn save(conn: &mut Connection, files_dir: &std::path::Path, v: &Value) -> Result<i64, String> {
    let date = v["date"].as_str().filter(|d| d.len() == 10).ok_or("Укажи дату сдачи анализов")?.to_string();
    let rows = v["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return Err("Нет ни одного результата".into());
    }
    let mut file: Option<String> = None;
    if let Some(src) = v["file"].as_str().filter(|s| !s.is_empty()) {
        let dir = files_dir.join("labs");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let name = format!("{date}_{}.pdf", store::now());
        if std::fs::copy(src, dir.join(&name)).is_ok() {
            file = Some(format!("labs/{name}"));
        }
    }
    let tx = conn.transaction().map_err(err)?;
    tx.execute(
        "INSERT INTO lab_reports (date, lab, file, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![date, v["lab"].as_str().unwrap_or(""), file, v["source"].as_str().unwrap_or("manual"), store::now()],
    )
    .map_err(err)?;
    let id = tx.last_insert_rowid();
    {
        let mut st = tx
            .prepare("INSERT INTO lab_results (report_id, date, key, name, value, value_text, cmp, flag, unit, lo, hi, ref_text, section, comment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)")
            .map_err(err)?;
        for r in rows {
            if r["skip"].as_bool() == Some(true) {
                continue;
            }
            let name = r["name"].as_str().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let key = r["key"].as_str().filter(|k| !k.is_empty()).map(String::from).or_else(|| key_of(&name).map(String::from));
            let (lo, hi) = match (r["lo"].as_f64(), r["hi"].as_f64()) {
                (None, None) => parse_ref(r["refText"].as_str().unwrap_or("")),
                x => x,
            };
            st.execute(params![
                id, date, key, name, r["value"].as_f64(), r["valueText"].as_str().unwrap_or(""), r["cmp"].as_str().unwrap_or(""), r["flag"].as_str().unwrap_or(""),
                r["unit"].as_str().unwrap_or(""), lo, hi, r["refText"].as_str().unwrap_or(""), r["section"].as_str().unwrap_or(""), r["comment"].as_str().unwrap_or("")
            ])
            .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(id)
}

pub fn delete(conn: &Connection, files_dir: &std::path::Path, id: i64) -> Result<(), String> {
    if let Ok(Some(f)) = conn.query_row("SELECT file FROM lab_reports WHERE id = ?1", params![id], |r| r.get::<_, Option<String>>(0)) {
        let _ = std::fs::remove_file(files_dir.join(f));
    }
    conn.execute("DELETE FROM lab_results WHERE report_id = ?1", params![id]).map_err(err)?;
    conn.execute("DELETE FROM lab_reports WHERE id = ?1", params![id]).map_err(err)?;
    Ok(())
}

// ---------- reading ----------

#[derive(Clone, Debug)]
pub struct Res {
    pub date: String,
    pub key: String,
    pub name: String,
    pub value: Option<f64>,
    pub text: String,
    pub cmp: String,
    pub flag: String,
    pub unit: String,
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    pub ref_text: String,
    pub comment: String,
}

fn norm_name(n: &str) -> String {
    format!("n:{}", lc(n).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// All results, oldest first, with known tests converted to their canonical unit.
pub fn results(conn: &Connection) -> Vec<Res> {
    let Ok(mut st) = conn.prepare("SELECT date, key, name, value, value_text, cmp, flag, unit, lo, hi, ref_text, comment FROM lab_results ORDER BY date, id") else {
        return Vec::new();
    };
    let rows: Vec<Res> = st
        .query_map([], |r| {
            Ok(Res {
                date: r.get(0)?,
                key: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                name: r.get(2)?,
                value: r.get(3)?,
                text: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                cmp: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                flag: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                unit: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                lo: r.get(8)?,
                hi: r.get(9)?,
                ref_text: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                comment: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
            })
        })
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    rows.into_iter()
        .map(|mut x| {
            if x.key.is_empty() {
                x.key = norm_name(&x.name);
            } else if let Some(v) = x.value {
                let (cv, cu) = canon(&x.key, v, &x.unit);
                let f = if v != 0.0 { cv / v } else { 1.0 };
                x.value = Some(cv);
                x.lo = x.lo.map(|l| l * f);
                x.hi = x.hi.map(|h| h * f);
                x.unit = cu;
            }
            x
        })
        .collect()
}

/// "low" / "high" / "ok" / "" (no range): the lab's range first, its «+ / −» mark otherwise.
pub fn status(r: &Res) -> &'static str {
    let Some(v) = r.value else {
        return match r.flag.as_str() {
            "+" => "high",
            "-" => "low",
            _ => "",
        };
    };
    if let Some(lo) = r.lo {
        if v < lo || (r.cmp == "<" && v <= lo) {
            return "low";
        }
    }
    if let Some(hi) = r.hi {
        if v > hi && r.cmp != "<" {
            return "high";
        }
    }
    match r.flag.as_str() {
        "+" => "high",
        "-" => "low",
        _ if r.lo.is_some() || r.hi.is_some() => "ok",
        _ => "",
    }
}

/// The latest result of a known test within `days` days before `today`.
pub fn latest(conn: &Connection, key: &str, today: &str, days: i64) -> Option<Res> {
    let from = add_days(today, -days);
    results(conn).into_iter().filter(|r| r.key == key && r.date >= from && r.value.is_some()).last()
}

// ---------- notes for training and food ----------

/// Workouts' heart load in the 3 days before a sample (date-1 .. date): (sum, longest minutes).
fn load_before(conn: &Connection, date: &str) -> (f64, f64) {
    let p = crate::engine_svc::profile(conn);
    let from = crate::health_svc::iso(&format!("{}T00:00:00Z", add_days(date, -3))).unwrap_or(0) - p.tz_offset_s;
    let to = crate::health_svc::iso(&format!("{date}T12:00:00Z")).unwrap_or(0) - p.tz_offset_s;
    let Ok(mut st) = conn.prepare(
        "SELECT COALESCE((SELECT m.load FROM activity_metrics m WHERE m.activity_id = activities.id), 0), COALESCE(moving_s, elapsed_s, 0) FROM va AS activities WHERE start_utc BETWEEN ?1 AND ?2",
    ) else {
        return (0.0, 0.0);
    };
    let v: Vec<(f64, f64)> = st.query_map(params![from, to], |r| Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    (v.iter().map(|x| x.0).sum(), v.iter().map(|x| x.1 / 60.0).fold(0.0, f64::max))
}

/// (level warn/info/bad, title, text, todo) for one result, or None when there is nothing to say.
pub fn note(conn: &Connection, r: &Res) -> Option<(&'static str, String, String, String)> {
    let st = status(r);
    let v = r.value?;
    let name = test(&r.key).map(|t| t.1.to_string()).unwrap_or_else(|| r.name.clone());
    let val = format!("{} {}", fmt(v), r.unit);
    let after_load = || {
        let (load, longest) = load_before(conn, &r.date);
        if load > 0.0 {
            format!(" За 3 дня до сдачи была нагрузка {} (самая длинная тренировка {} мин) — после тяжёлых и длинных тренировок такие показатели часто временно растут.", load.round(), longest.round())
        } else {
            String::new()
        }
    };
    let doc = "Обсуди с врачом.";
    let x: (&str, String, String) = match r.key.as_str() {
        "ferritin" if v < 15.0 => ("bad", format!("Ферритин {val} — запасы железа очень низкие."), "Выносливость падает, пульс на привычном темпе выше, восстановление медленнее. Не начинай железо сам — сначала врач: важно найти причину и подобрать дозу.".into()),
        "ferritin" if v < 30.0 => ("warn", format!("Ферритин {val} — запасы железа на нижней границе."), "Для бегунов ориентир — от 30–50: при нагрузках железо расходуется быстрее. В еду: говядина, печень, гречка, чечевица, вместе с овощами и фруктами с витамином C; чай и кофе — не сразу после еды.".into()),
        "hgb" if st == "low" => ("warn", format!("Гемоглобин {val} — ниже нормы лаборатории."), "Кислорода к мышцам доходит меньше: пульс выше на привычном темпе, тяжелее интенсивные тренировки. Интервалы лучше отложить до разговора с врачом.".into()),
        "vitd" if v < 20.0 => ("warn", format!("Витамин D {val} — дефицит (меньше 20)."), "Влияет на кости, мышцы и иммунитет; у бегунов — риск стрессовых переломов. Дозу добавки подбирает врач; в еду — жирная рыба, яйца.".into()),
        "vitd" if v < 30.0 => ("info", format!("Витамин D {val} — недостаточно (20–30)."), "Целевой уровень 30–60 нг/мл. Осенью и зимой солнца мало — обсуди с врачом профилактическую дозу.".into()),
        "vitd" if v > 100.0 => ("warn", format!("Витамин D {val} — больше 100."), "Это уже риск избытка: проверь дозы всех добавок с D. Обсуди с врачом.".into()),
        "b12" if st == "low" || v < 250.0 => ("warn", format!("Витамин B12 {val} — низко."), "Усталость, онемение и покалывание в руках и ногах. Источники — мясо, рыба, яйца, молочные продукты; дозу добавки — с врачом.".into()),
        "folate" if st == "low" => ("info", format!("Фолиевая кислота {val} — ниже нормы."), "В еду: зелень, шпинат, брокколи, бобовые, печень.".into()),
        "magnesium" if st == "low" => ("info", format!("Магний {val} — ниже нормы."), "Возможны судороги и плохой сон. В еду: гречка, овсянка, орехи, семечки, бобовые.".into()),
        "ck" if st == "high" => ("info", format!("КФК {val} — выше нормы."), format!("КФК выходит из мышц после нагрузки.{} Перед следующей сдачей — 2–3 дня без тяжёлых и длинных тренировок. Если и тогда высоко или есть боль в мышцах и тёмная моча — сразу к врачу.", after_load())),
        "ast" | "alt" if st == "high" => ("info", format!("{} {val} — выше нормы.", if r.key == "ast" { "АСТ" } else { "АЛТ" }), format!("{}{} Пересдай после нескольких спокойных дней; если остаётся высоким — {}", if r.key == "ast" { "АСТ бывает и в мышцах, растёт после нагрузки вместе с КФК." } else { "АЛТ больше про печень, но после очень тяжёлой нагрузки тоже растёт." }, after_load(), doc.to_lowercase())),
        "creatinine" if st == "high" => ("info", format!("Креатинин {val} — выше нормы."), format!("У спортсменов он выше из-за мышечной массы, креатина в добавках, белка и нагрузки накануне.{} Пей достаточно; если повторяется — {}", after_load(), doc.to_lowercase())),
        "urea" if st == "high" => ("info", format!("Мочевина {val} — выше нормы."), "Часто от много белка, мало воды или тяжёлой нагрузки накануне. Пей достаточно в день перед сдачей.".into()),
        "crp" if v > 10.0 => ("bad", format!("С-реактивный белок {val} — высокий."), "Похоже на воспаление или инфекцию: без интенсивных тренировок, пока не выяснишь причину. Обсуди с врачом.".into()),
        "crp" if v > 3.0 => ("warn", format!("С-реактивный белок {val} — повышен."), "Бывает при простуде, травме, после очень тяжёлой нагрузки. Пару дней — лёгкие тренировки; если держится — к врачу.".into()),
        "glucose" if st == "low" => ("info", format!("Глюкоза {val} — чуть ниже нормы."), "Натощак у бегунов так бывает после длинной тренировки или мало углеводов накануне. Если есть слабость и дрожь — обсуди с врачом.".into()),
        "glucose" if st == "high" => ("warn", format!("Глюкоза {val} — выше нормы."), "Сдавать нужно строго натощак; если было так — обсуди с врачом.".into()),
        "hba1c" if v >= 6.0 => ("warn", format!("Гликированный гемоглобин {val}."), "Это выше нормального уровня (до 6%). Обсуди с врачом.".into()),
        "ldl" if v > 3.4 => ("info", format!("ЛПНП {val} — выше целевого для умеренного риска (3,4)."), "Помогают клетчатка (овсянка, бобовые, овощи), рыба, орехи, меньше насыщенных жиров. Обсуди с врачом.".into()),
        "tg" if v >= 1.7 => ("info", format!("Триглицериды {val} — выше нормального (1,7)."), "Обычно снижаются от меньшего количества сладкого и алкоголя и регулярных тренировок.".into()),
        "hdl" if v < 1.03 => ("info", format!("ЛПВП {val} — низкий."), "Регулярная аэробная нагрузка и ненасыщенные жиры (рыба, орехи, масло) его поднимают.".into()),
        "tsh" if st != "ok" && !st.is_empty() => ("warn", format!("ТТГ {val} — {} нормы.", if st == "high" { "выше" } else { "ниже" }), "Щитовидная железа влияет на энергию, пульс и вес. Обсуди с врачом.".into()),
        "testo" if v < 8.0 => ("warn", format!("Тестостерон {val} — ниже 8."), "У спортсменов бывает от недосыпа, недостатка калорий при большой нагрузке, перетренированности. Обсуди с врачом.".into()),
        "testo" if v < 12.0 => ("info", format!("Тестостерон {val} — 8–12, пограничный."), "Чаще всего помогают сон, достаточно калорий и жиров, разгрузочные недели. Если есть усталость и падает форма — к врачу.".into()),
        "cortisol" if st == "high" => ("info", format!("Кортизол {val} — выше нормы."), "Бывает от стресса, недосыпа и тяжёлых тренировок накануне. Сравни с готовностью и стрессом в Kouros.".into()),
        "omega3" if v < 4.0 => ("warn", format!("Омега-3 индекс {val} — низкий."), "Жирная рыба (лосось, скумбрия, сельдь) 2–3 раза в неделю или омега-3 по согласованию с врачом.".into()),
        "omega3" if v < 8.0 => ("info", format!("Омега-3 индекс {val} — умеренный (4–8)."), "Цель — от 8: жирная рыба 2–3 раза в неделю.".into()),
        "potassium" | "sodium" | "calcium" if st != "ok" && !st.is_empty() => ("warn", format!("{name} {val} — {} нормы.", if st == "high" { "выше" } else { "ниже" }), "Электролиты важны для мышц и сердца. Обсуди с врачом.".into()),
        _ if st == "high" || st == "low" => ("info", format!("{name} {val} — {} нормы лаборатории.", if st == "high" { "выше" } else { "ниже" }), doc.into()),
        _ => return None,
    };
    let todo = if x.0 == "bad" { "Обсуди с врачом в ближайшее время.".to_string() } else { "Это не диагноз — обсуди результат с врачом.".to_string() };
    Some((x.0, x.1, x.2, todo))
}

fn fmt(v: f64) -> String {
    let s = if v.abs() >= 100.0 { format!("{v:.0}") } else if v.abs() >= 10.0 { format!("{v:.1}") } else { format!("{v:.2}") };
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

/// Findings of the latest result of every test (newest report first in priority).
pub fn findings(conn: &Connection, today: &str, days: i64) -> Vec<Value> {
    let from = add_days(today, -days);
    let all = results(conn);
    let mut last: BTreeMap<String, Res> = BTreeMap::new();
    for r in all.into_iter().filter(|r| r.date >= from) {
        last.insert(r.key.clone(), r);
    }
    let mut out: Vec<(u8, Value)> = last
        .values()
        .filter_map(|r| {
            let (lv, title, text, todo) = note(conn, r)?;
            let rank = match lv {
                "bad" => 0,
                "warn" => 1,
                _ => 2,
            };
            Some((rank, json!({"key": r.key, "date": r.date, "level": lv, "title": title, "text": text, "todo": todo})))
        })
        .collect();
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|x| x.1).collect()
}

/// The «Анализы» section: reports, every test with its series, findings.
pub fn summary(conn: &Connection, today: &str) -> Result<Value, String> {
    let reports: Vec<Value> = {
        let mut st = conn.prepare("SELECT r.id, r.date, r.lab, r.file, r.source, (SELECT count(*) FROM lab_results x WHERE x.report_id = r.id) FROM lab_reports r ORDER BY r.date DESC, r.id DESC").map_err(err)?;
        let v: Vec<Value> = st
            .query_map([], |r| {
                Ok(json!({"id": r.get::<_, i64>(0)?, "date": r.get::<_, String>(1)?, "lab": r.get::<_, Option<String>>(2)?, "file": r.get::<_, Option<String>>(3)?,
                          "source": r.get::<_, Option<String>>(4)?, "count": r.get::<_, i64>(5)?}))
            })
            .map_err(err)?
            .flatten()
            .collect();
        v
    };
    let all = results(conn);
    let mut by: BTreeMap<String, Vec<&Res>> = BTreeMap::new();
    for r in &all {
        by.entry(r.key.clone()).or_default().push(r);
    }
    let mut tests: Vec<Value> = by
        .iter()
        .map(|(k, v)| {
            let last = v.last().unwrap();
            let t = test(k);
            let series: Vec<Value> = v.iter().filter_map(|r| r.value.map(|x| json!([r.date, (x * 1000.0).round() / 1000.0]))).collect();
            json!({
                "key": k, "known": t.is_some(), "name": t.map(|t| t.1.to_string()).unwrap_or_else(|| last.name.clone()), "printed": last.name,
                "group": t.map(|t| t.2).unwrap_or("Другие"), "unit": last.unit,
                "latest": {"date": last.date, "value": last.value, "text": last.text, "lo": last.lo, "hi": last.hi, "ref": last.ref_text, "status": status(last), "comment": last.comment},
                "series": series, "count": v.len(),
                "note": note(conn, last).map(|(lv, title, text, todo)| json!({"level": lv, "title": title, "text": text, "todo": todo})),
            })
        })
        .collect();
    let order = ["Железо", "Кровь", "Витамины", "Минералы", "Мышцы и печень", "Почки", "Воспаление", "Сахар", "Липиды", "Гормоны", "Жирные кислоты", "Прочее", "Другие"];
    tests.sort_by_key(|t| (order.iter().position(|g| t["group"] == *g).unwrap_or(99), t["name"].as_str().unwrap_or("").to_string()));
    Ok(json!({"reports": reports, "tests": tests, "findings": findings(conn, today, 365), "groups": order}))
}

/// Short lines for the Claude package and the coach.
pub fn lines(conn: &Connection, today: &str) -> Vec<String> {
    let all = results(conn);
    let Some(last_date) = all.iter().map(|r| r.date.clone()).max() else { return Vec::new() };
    let mut out = vec![format!("Последние анализы: {last_date}")];
    let mut last: BTreeMap<String, Res> = BTreeMap::new();
    for r in all.into_iter() {
        last.insert(r.key.clone(), r);
    }
    for r in last.values() {
        let st = status(r);
        let known = test(&r.key).is_some();
        if st == "low" || st == "high" || (known && r.date == last_date) {
            out.push(format!(
                "{} ({}): {} {}{}{}",
                test(&r.key).map(|t| t.1.to_string()).unwrap_or_else(|| r.name.clone()),
                r.date,
                r.value.map(fmt).unwrap_or_else(|| r.text.clone()),
                r.unit,
                if r.ref_text.is_empty() { String::new() } else { format!(", норма {}", r.ref_text) },
                match st {
                    "low" => " — ниже нормы",
                    "high" => " — выше нормы",
                    _ => "",
                }
            ));
        }
    }
    let _ = today;
    out.truncate(40);
    out
}

/// CRP above 5 mg/l in the last 7 days → no intensity for the next 3 days (plan proposals).
pub fn crp_recent(conn: &Connection, today: &str) -> Option<(String, f64)> {
    latest(conn, "crp", today, 7).and_then(|r| r.value.filter(|v| *v > 5.0).map(|v| (r.date, v)))
}
