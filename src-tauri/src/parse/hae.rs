//! Health Auto Export (iOS app) files and pushes, std only (1.3).
//!
//! The JSON is read as a stream — a year of minute-by-minute data is gigabytes — so nothing but the
//! current metric is kept in memory. Samples are turned into the same metric names and formats the
//! rest of Kouros reads (Apple's names as freddy delivers them), so every calculation works
//! unchanged when a data group is switched to Health Auto Export.
//!
//! Output for the database side (`hae.rs`):
//! - `rows`: finished rows (food entries, weight, VO₂max, SpO₂ readings, sleep totals of
//!   summarised exports) — written as they are; the key makes a repeated import harmless;
//! - `bundles`: points of one metric, day and device (`[{t, v}]`), merged with what is stored and
//!   then turned into daily figures by [`derive`];
//! - `segs`: sleep stages, merged with stored ones and cut into nights by [`sleep_rows`];
//! - `food_days`: days whose food and drink totals must be summed again.

use super::time;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Read};

// ---------------------------------------------------------------- JSON stream

#[derive(Debug, Clone)]
pub enum Val {
    N(f64),
    S(String),
    Null,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct Row {
    pub f: Vec<(String, Val)>,
}

impl Row {
    pub fn num(&self, k: &str) -> Option<f64> {
        self.f.iter().find(|x| x.0 == k).and_then(|x| match &x.1 {
            Val::N(n) => Some(*n),
            Val::S(s) => s.trim().parse().ok(),
            _ => None,
        })
    }
    pub fn str(&self, k: &str) -> Option<&str> {
        self.f.iter().find(|x| x.0 == k).and_then(|x| match &x.1 {
            Val::S(s) => Some(s.as_str()),
            _ => None,
        })
    }
}

struct Lex<R: BufRead> {
    r: R,
    peeked: Option<u8>,
}

impl<R: BufRead> Lex<R> {
    fn next(&mut self) -> Result<Option<u8>, String> {
        if let Some(c) = self.peeked.take() {
            return Ok(Some(c));
        }
        let mut b = [0u8; 1];
        match self.r.read(&mut b) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(b[0])),
            Err(e) => Err(e.to_string()),
        }
    }
    fn peek(&mut self) -> Result<Option<u8>, String> {
        if self.peeked.is_none() {
            self.peeked = self.next()?;
        }
        Ok(self.peeked)
    }
    fn ws(&mut self) -> Result<Option<u8>, String> {
        loop {
            match self.peek()? {
                Some(b' ' | b'\n' | b'\r' | b'\t') => {
                    self.peeked = None;
                }
                Some(0xEF | 0xBB | 0xBF) => {
                    self.peeked = None; // BOM
                }
                other => return Ok(other),
            }
        }
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        match self.ws()? {
            Some(x) if x == c => {
                self.peeked = None;
                Ok(())
            }
            Some(x) => Err(format!("Файл повреждён: ждали «{}», а там «{}»", c as char, x as char)),
            None => Err("Файл обрывается раньше времени".into()),
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.next()?.ok_or("Файл обрывается")?;
            v = v * 16 + (c as char).to_digit(16).ok_or("Неверный символ в строке")?;
        }
        Ok(v)
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.next()?.ok_or("Файл обрывается внутри строки")?;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.next()?.ok_or("Файл обрывается")?;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' | b'f' => {}
                        b'u' => {
                            let mut u = self.hex4()?;
                            if (0xD800..0xDC00).contains(&u) {
                                // Surrogate pair.
                                if self.next()? == Some(b'\\') && self.next()? == Some(b'u') {
                                    let lo = self.hex4()?;
                                    u = 0x10000 + ((u - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                                }
                            }
                            let ch = char::from_u32(u).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        x => out.push(x),
                    }
                }
                x => out.push(x),
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
    fn scalar(&mut self) -> Result<Val, String> {
        match self.ws()? {
            Some(b'"') => Ok(Val::S(self.string()?)),
            Some(b'{') | Some(b'[') => {
                self.skip()?;
                Ok(Val::Other)
            }
            Some(_) => {
                let mut s = String::new();
                while let Some(c) = self.peek()? {
                    if c == b',' || c == b'}' || c == b']' || c.is_ascii_whitespace() {
                        break;
                    }
                    s.push(c as char);
                    self.peeked = None;
                }
                Ok(match s.as_str() {
                    "null" => Val::Null,
                    "true" | "false" => Val::Other,
                    _ => s.parse::<f64>().map(Val::N).unwrap_or(Val::Other),
                })
            }
            None => Err("Файл обрывается".into()),
        }
    }
    /// Skips any value.
    fn skip(&mut self) -> Result<(), String> {
        match self.ws()? {
            Some(b'"') => {
                self.string()?;
            }
            Some(b'{') => {
                self.peeked = None;
                if self.ws()? == Some(b'}') {
                    self.peeked = None;
                    return Ok(());
                }
                loop {
                    self.string()?;
                    self.expect(b':')?;
                    self.skip()?;
                    match self.ws()? {
                        Some(b',') => self.peeked = None,
                        Some(b'}') => {
                            self.peeked = None;
                            break;
                        }
                        _ => return Err("Файл повреждён (объект)".into()),
                    }
                }
            }
            Some(b'[') => {
                self.peeked = None;
                if self.ws()? == Some(b']') {
                    self.peeked = None;
                    return Ok(());
                }
                loop {
                    self.skip()?;
                    match self.ws()? {
                        Some(b',') => self.peeked = None,
                        Some(b']') => {
                            self.peeked = None;
                            break;
                        }
                        _ => return Err("Файл повреждён (список)".into()),
                    }
                }
            }
            _ => {
                self.scalar()?;
            }
        }
        Ok(())
    }
    /// Walks an object: `f` is called with each key and must consume the value.
    fn object(&mut self, mut f: impl FnMut(&mut Self, String) -> Result<(), String>) -> Result<(), String> {
        self.expect(b'{')?;
        if self.ws()? == Some(b'}') {
            self.peeked = None;
            return Ok(());
        }
        loop {
            let k = self.string()?;
            self.expect(b':')?;
            f(self, k)?;
            match self.ws()? {
                Some(b',') => self.peeked = None,
                Some(b'}') => {
                    self.peeked = None;
                    return Ok(());
                }
                _ => return Err("Файл повреждён (объект)".into()),
            }
        }
    }
    fn array(&mut self, mut f: impl FnMut(&mut Self) -> Result<(), String>) -> Result<(), String> {
        if self.ws()? != Some(b'[') {
            return self.skip();
        }
        self.peeked = None;
        if self.ws()? == Some(b']') {
            self.peeked = None;
            return Ok(());
        }
        loop {
            f(self)?;
            match self.ws()? {
                Some(b',') => self.peeked = None,
                Some(b']') => {
                    self.peeked = None;
                    return Ok(());
                }
                _ => return Err("Файл повреждён (список)".into()),
            }
        }
    }
}

/// Reads an export and calls `sink` for each metric: (name, units, rows of that metric).
pub fn stream<R: Read>(r: R, sink: &mut dyn FnMut(&str, &str, Vec<Row>) -> Result<(), String>) -> Result<(), String> {
    let mut lx = Lex { r: std::io::BufReader::with_capacity(1 << 16, r), peeked: None };
    if lx.ws()? != Some(b'{') {
        return Err("Это не выгрузка Health Auto Export (ждали JSON)".into());
    }
    let mut seen = false;
    lx.object(|lx, k| {
        if k != "data" {
            return lx.skip();
        }
        if lx.ws()? != Some(b'{') {
            return lx.skip();
        }
        lx.object(|lx, k| {
            if k != "metrics" {
                return lx.skip(); // workouts, ECG, medications… are not read here
            }
            seen = true;
            lx.array(|lx| {
                if lx.ws()? != Some(b'{') {
                    return lx.skip();
                }
                let mut name = String::new();
                let mut units = String::new();
                let mut rows: Vec<Row> = Vec::new();
                lx.object(|lx, k| match k.as_str() {
                    "name" => {
                        name = lx.string()?;
                        Ok(())
                    }
                    "units" => {
                        units = match lx.scalar()? {
                            Val::S(s) => s,
                            _ => String::new(),
                        };
                        Ok(())
                    }
                    "data" => lx.array(|lx| {
                        if lx.ws()? != Some(b'{') {
                            return lx.skip();
                        }
                        let mut row = Row::default();
                        lx.object(|lx, k| {
                            let v = lx.scalar()?;
                            row.f.push((k, v));
                            Ok(())
                        })?;
                        rows.push(row);
                        Ok(())
                    }),
                    _ => lx.skip(),
                })?;
                sink(&name, &units, rows)
            })
        })
    })?;
    if !seen {
        return Err("В файле нет данных о здоровье (раздела metrics). Выгрузи «Показатели здоровья» в формате JSON".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- times

/// "2026-10-21 05:00:45 +0300", "2026-10-21T05:00:45+03:00", "…Z" → (unix, offset seconds).
pub fn when(s: &str) -> Option<(i64, i64)> {
    let s = s.trim();
    let n = |a: usize, b: usize| -> Option<i64> { s.get(a..b)?.parse().ok() };
    let (y, mo, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, se) = if s.len() >= 19 { (n(11, 13)?, n(14, 16)?, n(17, 19)?) } else { (0, 0, 0) };
    let local = time::unix(y, mo, d, h, mi, se);
    let mut rest = s.get(19..).unwrap_or("");
    if rest.starts_with('.') {
        let end = rest[1..].find(|c: char| !c.is_ascii_digit()).map(|i| i + 1).unwrap_or(rest.len());
        rest = &rest[end..];
    }
    let rest = rest.trim();
    let off = if rest.starts_with('+') || rest.starts_with('-') {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
        let oh: i64 = digits.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
        let om: i64 = digits.get(2..4).and_then(|x| x.parse().ok()).unwrap_or(0);
        sign * (oh * 3600 + om * 60)
    } else {
        0
    };
    Some((local - off, off))
}

/// ISO with the local offset ("2026-10-21T05:00:45+03:00") — the stored hypnogram keeps the
/// offset so a night read back later still knows its local day.
pub fn iso_off(t: i64, off: i64) -> String {
    let l = t + off;
    let days = l.div_euclid(86400);
    let secs = l.rem_euclid(86400);
    let (y, m, d) = time::civil_from_days(days);
    let sign = if off < 0 { '-' } else { '+' };
    let a = off.abs();
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60, a / 3600, a % 3600 / 60)
}

fn local_day(t: i64, off: i64) -> String {
    time::ymd(t, off)
}

// ---------------------------------------------------------------- mapping

#[derive(Clone, Copy, Debug, PartialEq)]
enum Conv {
    None,
    KjToKcal,
    KmhToMs,
    CmToM,
    LbToKg,
    FToC,
    Pct,
    HrToS,
}

fn conv(c: Conv, units: &str, v: f64) -> f64 {
    let u = units.to_lowercase();
    match c {
        Conv::KjToKcal if u == "kj" => v / 4.184,
        Conv::KmhToMs if u.starts_with("km") => v / 3.6,
        Conv::KmhToMs if u.starts_with("mi") => v * 0.44704,
        Conv::CmToM if u == "cm" => v / 100.0,
        Conv::CmToM if u == "in" => v * 0.0254,
        Conv::LbToKg if u.starts_with("lb") => v * 0.453592,
        Conv::FToC if u.contains('f') => (v - 32.0) * 5.0 / 9.0,
        Conv::Pct if v <= 1.0 => v * 100.0,
        Conv::HrToS if u.starts_with("hr") || u == "h" => v * 3600.0,
        Conv::HrToS if u.starts_with("min") => v * 60.0,
        _ => v,
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Food nutrient: food_entry_<key>, daily dietary_<key>_total.
    Food(&'static str),
    /// A drink or a dose: entry metric (water, caffeine, alcohol).
    Drink(&'static str),
    /// Per-second amounts summed into minutes (steps, energy).
    PerMin(&'static str, Conv),
    /// Readings kept as a day bundle `[{t, v}]` (`Avg` for heart rate).
    Points(&'static str, Conv),
    /// Readings stored one row each.
    Direct(&'static str, Conv),
    Sleep,
    /// Known, not used by Kouros.
    Skip,
}

fn kind(name: &str) -> Option<Kind> {
    use Kind::*;
    let n = name.to_lowercase();
    let food = |k: &'static str| Some(Food(k));
    Some(match n.as_str() {
        "dietary_energy" | "dietary_energy_consumed" => Food("energy"),
        "protein" | "dietary_protein" => return food("protein"),
        "carbohydrates" | "dietary_carbohydrates" => return food("carbohydrates"),
        "total_fat" | "dietary_fat_total" => return food("fat_total"),
        "saturated_fat" => return food("fat_saturated"),
        "monounsaturated_fat" => return food("fat_mono"),
        "polyunsaturated_fat" => return food("fat_poly"),
        "dietary_sugar" | "sugar" => return food("sugar"),
        "fiber" | "dietary_fiber" => return food("fiber"),
        "cholesterol" | "dietary_cholesterol" => return food("cholesterol"),
        "sodium" | "potassium" | "calcium" | "iron" | "magnesium" | "zinc" | "phosphorus" | "copper" | "manganese" | "selenium" | "chromium"
        | "molybdenum" | "iodine" | "chloride" | "biotin" | "folate" | "niacin" | "riboflavin" | "thiamin" | "pantothenic_acid" | "vitamin_a"
        | "vitamin_b6" | "vitamin_b12" | "vitamin_c" | "vitamin_d" | "vitamin_e" | "vitamin_k" => {
            return Some(Food(match n.as_str() {
                "sodium" => "sodium",
                "potassium" => "potassium",
                "calcium" => "calcium",
                "iron" => "iron",
                "magnesium" => "magnesium",
                "zinc" => "zinc",
                "phosphorus" => "phosphorus",
                "copper" => "copper",
                "manganese" => "manganese",
                "selenium" => "selenium",
                "chromium" => "chromium",
                "molybdenum" => "molybdenum",
                "iodine" => "iodine",
                "chloride" => "chloride",
                "biotin" => "biotin",
                "folate" => "folate",
                "niacin" => "niacin",
                "riboflavin" => "riboflavin",
                "thiamin" => "thiamin",
                "pantothenic_acid" => "pantothenic_acid",
                "vitamin_a" => "vitamin_a",
                "vitamin_b6" => "vitamin_b6",
                "vitamin_b12" => "vitamin_b12",
                "vitamin_c" => "vitamin_c",
                "vitamin_d" => "vitamin_d",
                "vitamin_e" => "vitamin_e",
                _ => "vitamin_k",
            }))
        }
        "dietary_water" | "water" => Drink("drink_entry_volume"),
        "caffeine" | "dietary_caffeine" => Drink("food_entry_caffeine"),
        "number_of_alcoholic_beverages" | "alcoholic_beverages" | "alcohol_consumption" | "number_of_alcoholic_drinks" => Drink("drink_entry_alcohol_units"),
        "step_count" | "steps" => PerMin("steps_raw", Conv::None),
        "active_energy" | "active_energy_burned" => PerMin("active_energy_raw", Conv::KjToKcal),
        "basal_energy_burned" | "resting_energy" | "basal_energy" => PerMin("basal_energy_raw", Conv::KjToKcal),
        "heart_rate" => Points("heart_rate_raw", Conv::None),
        "heart_rate_variability" | "heart_rate_variability_sdnn" => Points("hrv_sdnn_raw", Conv::None),
        "heart_rate_variability_rmssd" => Points("hrv_rmssd_raw", Conv::None),
        "resting_heart_rate" => Points("resting_heart_rate_raw", Conv::None),
        "walking_heart_rate_average" => Points("walking_hr_raw", Conv::None),
        "respiratory_rate" => Points("respiratory_rate_raw", Conv::None),
        "blood_oxygen_saturation" | "oxygen_saturation" => Points("blood_oxygen_raw", Conv::Pct),
        "apple_sleeping_wrist_temperature" | "sleeping_wrist_temperature" | "wrist_temperature" => Points("wrist_temp_raw", Conv::FToC),
        "breathing_disturbances" | "sleeping_breathing_disturbances" => Points("breathing_raw", Conv::None),
        "running_power" => Points("running_power_raw", Conv::None),
        "running_speed" => Points("running_speed_raw", Conv::KmhToMs),
        "running_stride_length" => Points("running_stride_length_raw", Conv::CmToM),
        "running_ground_contact_time" => Points("running_ground_contact_time_raw", Conv::None),
        "running_vertical_oscillation" => Points("running_vertical_oscillation_raw", Conv::None),
        "walking_speed" => Points("walking_speed_raw", Conv::KmhToMs),
        "walking_step_length" => Points("walking_step_length_raw", Conv::CmToM),
        "walking_asymmetry_percentage" => Points("walking_asymmetry_raw", Conv::Pct),
        "walking_double_support_percentage" => Points("walking_double_support_raw", Conv::Pct),
        "vo2_max" => Direct("vo2_max", Conv::None),
        "weight_body_mass" | "body_mass" | "weight" => Direct("body_mass", Conv::LbToKg),
        "body_fat_percentage" => Direct("body_fat_pct", Conv::Pct),
        "lean_body_mass" => Direct("lean_body_mass", Conv::LbToKg),
        "body_mass_index" => Direct("bmi", Conv::None),
        "heart_rate_recovery_one_minute" | "cardio_recovery" | "heart_rate_recovery" => Direct("hr_recovery_1min", Conv::None),
        "sleep_analysis" => Sleep,
        "apple_stand_hour" | "apple_stand_time" | "apple_exercise_time" | "walking_running_distance" | "flights_climbed" | "environmental_audio_exposure"
        | "headphone_audio_exposure" | "physical_effort" | "stair_speed_up" | "stair_speed_down" | "six_minute_walking_test_distance"
        | "time_in_daylight" | "apple_move_time" | "cycling_distance" | "swimming_distance" | "swimming_stroke_count" | "height"
        | "body_temperature" | "walking_steadiness" | "handwashing" | "toothbrushing" | "mindful_minutes" | "number_of_times_fallen"
        | "blood_alcohol_content" | "uv_exposure" | "atrial_fibrillation_burden" | "cardio_fitness" => Skip,
        _ => return None,
    })
}

/// Daily figures computed from a day's points: bundle metric → (daily metric, sum?).
pub const DERIVE: &[(&str, &str, bool)] = &[
    ("steps_raw", "steps_total", true),
    ("active_energy_raw", "active_energy_total", true),
    ("basal_energy_raw", "basal_energy_total", true),
    ("heart_rate_raw", "heart_rate_merged_avg", false),
    ("hrv_sdnn_raw", "hrv_sdnn_merged_avg", false),
    ("hrv_rmssd_raw", "hrv_rmssd_merged_avg", false),
    ("resting_heart_rate_raw", "resting_heart_rate_merged_avg", false),
    ("walking_hr_raw", "walking_hr_avg_merged_value", false),
    ("respiratory_rate_raw", "respiratory_rate_merged_avg", false),
    ("blood_oxygen_raw", "blood_oxygen_avg", false),
    ("wrist_temp_raw", "wrist_temp", false),
    ("breathing_raw", "sleeping_breathing_disturbances", false),
    ("walking_speed_raw", "walking_speed_merged_avg", false),
    ("walking_asymmetry_raw", "walking_asymmetry_merged_avg", false),
    ("walking_step_length_raw", "walking_step_length_merged_avg", false),
    ("walking_double_support_raw", "walking_double_support_merged_avg", false),
];

/// All daily-figure metrics (for removing stale ones is never needed — they are overwritten).
pub fn derive_rule(metric: &str) -> Option<(&'static str, bool)> {
    DERIVE.iter().find(|x| x.0 == metric).map(|x| (x.1, x.2))
}

/// Food keys that have daily totals.
pub const FOOD_KEYS: &[&str] = &[
    "energy", "protein", "carbohydrates", "fat_total", "fat_saturated", "fat_mono", "fat_poly", "sugar", "fiber", "cholesterol", "sodium",
    "potassium", "calcium", "iron", "magnesium", "zinc", "phosphorus", "copper", "manganese", "selenium", "chromium", "molybdenum", "iodine",
    "chloride", "biotin", "folate", "niacin", "riboflavin", "thiamin", "pantothenic_acid", "vitamin_a", "vitamin_b6", "vitamin_b12",
    "vitamin_c", "vitamin_d", "vitamin_e", "vitamin_k",
];

fn unit_of(metric: &str, units: &str) -> String {
    match metric {
        "steps_raw" | "steps_total" => "count".into(),
        "active_energy_raw" | "basal_energy_raw" | "active_energy_total" | "basal_energy_total" => "kcal".into(),
        "heart_rate_raw" | "resting_heart_rate_raw" | "walking_hr_raw" => "bpm".into(),
        "hrv_sdnn_raw" => "ms_sdnn".into(),
        "hrv_rmssd_raw" => "ms_rmssd".into(),
        "blood_oxygen_raw" | "blood_oxygen" | "body_fat_pct" | "walking_asymmetry_raw" | "walking_double_support_raw" => "percent".into(),
        "wrist_temp_raw" => "degC".into(),
        "running_speed_raw" | "walking_speed_raw" => "m/s".into(),
        "running_stride_length_raw" | "walking_step_length_raw" => "m".into(),
        "body_mass" | "lean_body_mass" => "kg".into(),
        "vo2_max" => "ml/kg/min".into(),
        _ => units.to_string(),
    }
}

// ---------------------------------------------------------------- output

/// A finished row (provider is added by the caller).
#[derive(Debug, Clone)]
pub struct Out {
    pub metric: String,
    pub date: String,
    pub ts: String,
    pub device: String,
    pub source_id: String,
    pub num: Option<f64>,
    pub txt: Option<String>,
    pub unit: String,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    pub metric: String,
    pub date: String,
    pub device: String,
    pub unit: String,
    pub points: Vec<(i64, f64)>,
}

#[derive(Debug, Clone)]
pub struct Seg {
    pub device: String,
    pub stage: String,
    pub a: i64,
    pub b: i64,
    pub off: i64,
}

#[derive(Debug, Default)]
pub struct Batch {
    pub rows: Vec<Out>,
    pub bundles: Vec<Bundle>,
    pub segs: Vec<Seg>,
    pub food_days: BTreeSet<String>,
    /// Earliest and latest local day seen.
    pub first: Option<String>,
    pub last: Option<String>,
    pub samples: usize,
    pub unknown: BTreeMap<String, usize>,
}

impl Batch {
    fn day(&mut self, d: &str) {
        if self.first.as_deref().map(|f| d < f).unwrap_or(true) {
            self.first = Some(d.to_string());
        }
        if self.last.as_deref().map(|l| d > l).unwrap_or(true) {
            self.last = Some(d.to_string());
        }
    }
}

fn device_of(r: &Row) -> String {
    let s = r.str("source").unwrap_or("").trim();
    if s.is_empty() { "Merged".to_string() } else { s.to_string() }
}

fn stage_of(v: &str) -> Option<&'static str> {
    let v = v.trim().to_lowercase();
    Some(match v.as_str() {
        "core" | "asleepcore" | "основная" | "основной" | "лёгкий" | "легкий" | "light" => "core",
        "deep" | "asleepdeep" | "глубокий" | "глубокая" => "deep",
        "rem" | "asleeprem" | "бдг" | "быстрый" => "rem",
        "awake" | "бодрствование" | "пробуждение" => "awake",
        "asleep" | "asleepunspecified" | "сон" | "unspecified" => "core",
        _ => return None, // in bed
    })
}

/// Turns the rows of one metric into a batch. `rows` of other metrics are ignored.
pub fn convert(name: &str, units: &str, rows: &[Row], out: &mut Batch) {
    let Some(k) = kind(name) else {
        *out.unknown.entry(name.to_string()).or_insert(0) += rows.len();
        return;
    };
    let time_of = |r: &Row| -> Option<(i64, i64)> { r.str("start").or(r.str("date")).or(r.str("startDate")).and_then(when) };
    match k {
        Kind::Skip => {}
        Kind::Food(key) => {
            // One row per (time, product); two identical products at one moment are added up.
            let mut acc: BTreeMap<(i64, String, String), (f64, i64)> = BTreeMap::new();
            for r in rows {
                let (Some((t, off)), Some(q)) = (time_of(r), r.num("qty")) else { continue };
                let q = if key == "energy" { conv(Conv::KjToKcal, units, q) } else { q };
                let food = r.str("foodType").unwrap_or("").trim().to_string();
                let e = acc.entry((t, device_of(r), food)).or_insert((0.0, off));
                e.0 += q;
            }
            for ((t, dev, food), (q, off)) in acc {
                let d = local_day(t, off);
                out.day(&d);
                out.food_days.insert(d.clone());
                out.samples += 1;
                let ts = time::iso(t);
                if !food.is_empty() {
                    out.rows.push(Out { metric: "food_entry_name".into(), date: d.clone(), ts: ts.clone(), device: dev.clone(), source_id: food.clone(), num: None, txt: Some(food.clone()), unit: String::new() });
                }
                let unit = if key == "energy" { "kcal".to_string() } else { units.to_string() };
                out.rows.push(Out { metric: format!("food_entry_{key}"), date: d, ts, device: dev, source_id: food, num: Some(q), txt: None, unit });
            }
        }
        Kind::Drink(metric) => {
            let mut acc: BTreeMap<(i64, String, String), (f64, i64)> = BTreeMap::new();
            for r in rows {
                let (Some((t, off)), Some(q)) = (time_of(r), r.num("qty")) else { continue };
                let q = if metric == "drink_entry_volume" && units.to_lowercase().starts_with("fl") { q * 29.5735 } else if metric == "drink_entry_volume" && units == "L" { q * 1000.0 } else { q };
                let e = acc.entry((t, device_of(r), r.str("foodType").unwrap_or("").trim().to_string())).or_insert((0.0, off));
                e.0 += q;
            }
            for ((t, dev, food), (q, off)) in acc {
                let d = local_day(t, off);
                out.day(&d);
                out.food_days.insert(d.clone());
                out.samples += 1;
                let unit = match metric {
                    "drink_entry_volume" => "mL",
                    "food_entry_caffeine" => "mg",
                    _ => "count",
                };
                out.rows.push(Out { metric: metric.into(), date: d, ts: time::iso(t), device: dev, source_id: food, num: Some(q), txt: None, unit: unit.into() });
            }
        }
        Kind::PerMin(metric, c) => {
            let mut acc: BTreeMap<(String, String), BTreeMap<i64, f64>> = BTreeMap::new();
            for r in rows {
                let (Some((t, off)), Some(q)) = (time_of(r), r.num("qty")) else { continue };
                let q = conv(c, units, q);
                let d = local_day(t, off);
                *acc.entry((d, device_of(r))).or_default().entry(t - t.rem_euclid(60)).or_insert(0.0) += q;
                out.samples += 1;
            }
            for ((d, dev), m) in acc {
                out.day(&d);
                out.bundles.push(Bundle { metric: metric.into(), date: d, device: dev, unit: unit_of(metric, units), points: m.into_iter().map(|(t, v)| (t, (v * 1000.0).round() / 1000.0)).collect() });
            }
        }
        Kind::Points(metric, c) => {
            let mut acc: BTreeMap<(String, String), Vec<(i64, f64)>> = BTreeMap::new();
            for r in rows {
                let Some((t, off)) = time_of(r) else { continue };
                let Some(q) = r.num("Avg").or(r.num("qty")) else { continue };
                let q = conv(c, units, q);
                let d = local_day(t, off);
                acc.entry((d, device_of(r))).or_default().push((t, q));
                out.samples += 1;
                if metric == "blood_oxygen_raw" {
                    // Single readings too: the night SpO₂ is the median of readings while asleep.
                    out.rows.push(Out { metric: "blood_oxygen".into(), date: local_day(t, off), ts: time::iso(t), device: device_of(r), source_id: String::new(), num: Some(q), txt: None, unit: "percent".into() });
                }
            }
            for ((d, dev), v) in acc {
                out.day(&d);
                out.bundles.push(Bundle { metric: metric.into(), date: d, device: dev, unit: unit_of(metric, units), points: v });
            }
        }
        Kind::Direct(metric, c) => {
            for r in rows {
                let (Some((t, off)), Some(q)) = (time_of(r), r.num("qty")) else { continue };
                let d = local_day(t, off);
                out.day(&d);
                out.samples += 1;
                out.rows.push(Out { metric: metric.into(), date: d, ts: time::iso(t), device: device_of(r), source_id: String::new(), num: Some(conv(c, units, q)), txt: None, unit: unit_of(metric, units) });
            }
        }
        Kind::Sleep => {
            for r in rows {
                let dev = device_of(r);
                if let Some(v) = r.str("value") {
                    // Not summarised: one row per stage.
                    let Some(stage) = stage_of(v) else { continue };
                    let a = r.str("startDate").or(r.str("start")).and_then(when);
                    let b = r.str("endDate").or(r.str("end")).and_then(when);
                    if let (Some((a, off)), Some((b, _))) = (a, b) {
                        if b > a {
                            out.samples += 1;
                            out.segs.push(Seg { device: dev, stage: stage.into(), a, b, off });
                        }
                    }
                    continue;
                }
                // Summarised: night totals only (no minute-by-minute picture).
                let end = r.str("sleepEnd").or(r.str("inBedEnd")).or(r.str("date")).and_then(when);
                let start = r.str("sleepStart").or(r.str("inBedStart")).and_then(when);
                let Some((te, off)) = end else { continue };
                let d = local_day(te, off);
                let ts = time::iso(start.map(|x| x.0).unwrap_or(te));
                let mut any = false;
                for (k, m) in [("core", "sleep_analysis_core_seconds"), ("deep", "sleep_analysis_deep_seconds"), ("rem", "sleep_analysis_rem_seconds"), ("awake", "sleep_analysis_awake_seconds")] {
                    let mut v = r.num(k);
                    if k == "core" && v.unwrap_or(0.0) == 0.0 {
                        // Older watches: only "asleep".
                        v = r.num("asleep").filter(|x| *x > 0.0).or(v);
                    }
                    if let Some(v) = v {
                        any = true;
                        out.rows.push(Out { metric: m.into(), date: d.clone(), ts: ts.clone(), device: dev.clone(), source_id: String::new(), num: Some(conv(Conv::HrToS, units, v)), txt: None, unit: "s".into() });
                    }
                }
                if any {
                    out.day(&d);
                    out.samples += 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- after merging

/// Daily figure(s) of one bundle metric and day from every device's points.
pub fn derive(metric: &str, date: &str, devices: &[(String, Vec<(i64, f64)>)]) -> Option<Out> {
    let (daily, sum) = derive_rule(metric)?;
    let num = if sum {
        // HealthKit counts overlapping devices once; the merged stream if there is one, else the
        // device with the most.
        if let Some((_, p)) = devices.iter().find(|d| d.0 == "Merged") {
            p.iter().map(|x| x.1).sum::<f64>()
        } else {
            devices.iter().map(|(_, p)| p.iter().map(|x| x.1).sum::<f64>()).fold(0.0, f64::max)
        }
    } else {
        let all: Vec<f64> = devices.iter().flat_map(|(_, p)| p.iter().map(|x| x.1)).collect();
        if all.is_empty() {
            return None;
        }
        all.iter().sum::<f64>() / all.len() as f64
    };
    let num = (num * 100.0).round() / 100.0;
    Some(Out { metric: daily.into(), date: date.into(), ts: date.into(), device: "Merged".into(), source_id: String::new(), num: Some(num), txt: None, unit: unit_of(daily, "") })
}

/// `[{"t": "…Z", "v": n}]` of sorted points.
pub fn points_json(p: &[(i64, f64)]) -> String {
    let mut s = String::with_capacity(p.len() * 40 + 2);
    s.push('[');
    for (i, (t, v)) in p.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let v = if v.fract() == 0.0 { format!("{}", *v as i64) } else { format!("{}", (v * 1000.0).round() / 1000.0) };
        s.push_str(&format!("{{\"t\":\"{}\",\"v\":{}}}", time::iso(*t), v));
    }
    s.push(']');
    s
}

/// Sorted, one value per timestamp (the later one wins).
pub fn merge_points(old: Vec<(i64, f64)>, new: &[(i64, f64)]) -> Vec<(i64, f64)> {
    let mut m: BTreeMap<i64, f64> = old.into_iter().collect();
    for (t, v) in new {
        m.insert(*t, *v);
    }
    m.into_iter().collect()
}

/// Nights (and naps) of one device from its stages: a break longer than an hour starts a new
/// sleep. Each gives a hypnogram (`sleep_analysis_raw`, stages with the local offset) and the four
/// totals, dated by the local day it ended on.
pub fn sleep_rows(mut segs: Vec<Seg>) -> Vec<Out> {
    let mut out = Vec::new();
    segs.sort_by(|a, b| a.device.cmp(&b.device).then(a.a.cmp(&b.a)).then(a.b.cmp(&b.b)));
    segs.dedup_by(|b, a| a.device == b.device && a.a == b.a && a.b == b.b);
    let mut groups: Vec<Vec<Seg>> = Vec::new();
    for s in segs {
        match groups.last_mut() {
            Some(g) if g[0].device == s.device && s.a <= g.iter().map(|x| x.b).max().unwrap_or(0) + 3600 => g.push(s),
            _ => groups.push(vec![s]),
        }
    }
    for g in groups {
        let start = g.iter().map(|x| x.a).min().unwrap_or(0);
        let end = g.iter().map(|x| x.b).max().unwrap_or(0);
        let last = g.iter().max_by_key(|x| x.b).unwrap();
        let date = local_day(end, last.off);
        let dev = g[0].device.clone();
        let ts = time::iso(start);
        let mut tot: BTreeMap<&str, f64> = BTreeMap::new();
        let mut js = String::from("[");
        for (i, s) in g.iter().enumerate() {
            if i > 0 {
                js.push(',');
            }
            js.push_str(&format!("{{\"stage\":\"{}\",\"start\":\"{}\",\"end\":\"{}\"}}", s.stage, iso_off(s.a, s.off), iso_off(s.b, s.off)));
            let k = match s.stage.as_str() {
                "deep" => "deep",
                "rem" => "rem",
                "awake" => "awake",
                _ => "core",
            };
            *tot.entry(k).or_insert(0.0) += (s.b - s.a) as f64;
        }
        js.push(']');
        out.push(Out { metric: "sleep_analysis_raw".into(), date: date.clone(), ts: ts.clone(), device: dev.clone(), source_id: String::new(), num: None, txt: Some(js), unit: String::new() });
        for k in ["core", "deep", "rem", "awake"] {
            out.push(Out { metric: format!("sleep_analysis_{k}_seconds"), date: date.clone(), ts: ts.clone(), device: dev.clone(), source_id: String::new(), num: Some(tot.get(k).copied().unwrap_or(0.0)), txt: None, unit: "s".into() });
        }
    }
    out
}

/// Stored hypnogram text back into stages (for merging with new ones).
pub fn segs_from_txt(device: &str, txt: &str) -> Vec<Seg> {
    // Our own format only: {"stage":"core","start":"…","end":"…"}.
    let mut out = Vec::new();
    for part in txt.split('}') {
        let get = |k: &str| -> Option<String> {
            let i = part.find(&format!("\"{k}\":\""))? + k.len() + 4;
            let j = part[i..].find('"')? + i;
            Some(part[i..j].to_string())
        };
        if let (Some(st), Some(a), Some(b)) = (get("stage"), get("start"), get("end")) {
            if let (Some((a, off)), Some((b, _))) = (when(&a), when(&b)) {
                out.push(Seg { device: device.to_string(), stage: st, a, b, off });
            }
        }
    }
    out
}

/// Points of a stored bundle (our own format, `{"t":"…","v":n}`).
pub fn points_from_txt(txt: &str) -> Vec<(i64, f64)> {
    let mut out = Vec::new();
    for part in txt.split('}') {
        let Some(i) = part.find("\"t\":\"") else { continue };
        let i = i + 5;
        let Some(j) = part[i..].find('"') else { continue };
        let t = time::parse_iso(&part[i..i + j]);
        let Some(k) = part.find("\"v\":") else { continue };
        let v: Option<f64> = part[k + 4..].trim().trim_end_matches(',').parse().ok();
        if let (Some(t), Some(v)) = (t, v) {
            out.push((t, v));
        }
    }
    out
}

/// Whole file (or push body) → one batch per metric through `f`.
pub fn read_all<R: Read>(r: R, f: &mut dyn FnMut(Batch) -> Result<(), String>) -> Result<(), String> {
    stream(r, &mut |name, units, rows| {
        let mut b = Batch::default();
        convert(name, units, &rows, &mut b);
        f(b)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        let (t, off) = when("2026-10-21 05:00:45 +0300").unwrap();
        assert_eq!(off, 10800);
        assert_eq!(time::iso(t), "2026-10-21T02:00:45Z");
        assert_eq!(iso_off(t, off), "2026-10-21T05:00:45+03:00");
        assert_eq!(when("2026-10-21T02:00:45Z").unwrap().0, t);
    }

    #[test]
    fn small() {
        let js = r#"{"data":{"metrics":[{"name":"heart_rate","units":"count/min","data":[{"date":"2026-10-21 00:00:31 +0300","Avg":88.5,"Min":88,"Max":89,"source":"Ultra 4"}]},
        {"name":"sleep_analysis","units":"hr","data":[{"startDate":"2026-10-21 05:00:00 +0300","endDate":"2026-10-21 06:00:00 +0300","value":"Глубокий","source":"Ultra 4"},
        {"startDate":"2026-10-21 06:00:00 +0300","endDate":"2026-10-21 06:30:00 +0300","value":"БДГ","source":"Ultra 4"}]},
        {"name":"protein","units":"g","data":[{"date":"2026-10-21 12:00:00 +0300","qty":10,"foodType":"Гречка в","source":"Yazio"}]}],"workouts":[{"a":[1,2,{"b":null}]}]}}"#;
        let mut all = Batch::default();
        read_all(js.as_bytes(), &mut |b| {
            all.rows.extend(b.rows);
            all.bundles.extend(b.bundles);
            all.segs.extend(b.segs);
            Ok(())
        })
        .unwrap();
        assert_eq!(all.bundles.len(), 1);
        assert_eq!(all.bundles[0].points[0].1, 88.5);
        assert_eq!(all.segs.len(), 2);
        assert_eq!(all.rows.iter().find(|r| r.metric == "food_entry_name").unwrap().txt.as_deref(), Some("Гречка в"));
        let s = sleep_rows(all.segs.clone());
        assert_eq!(s.len(), 5);
        assert_eq!(s.iter().find(|r| r.metric == "sleep_analysis_deep_seconds").unwrap().num, Some(3600.0));
        let back = segs_from_txt("Ultra 4", s[0].txt.as_deref().unwrap());
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].off, 10800);
        let p = points_json(&[(100, 1.5), (160, 2.0)]);
        assert_eq!(points_from_txt(&p), vec![(100, 1.5), (160, 2.0)]);
    }
}
