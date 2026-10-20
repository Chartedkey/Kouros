//! CSV reader (RFC 4180: quoted fields, embedded commas, quotes and newlines)
//! and the mapping of Strava's `activities.csv` archive file.

use super::{sport_from_strava, time::parse_strava_csv_date, Parsed};

pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Column lookup by header name; Strava repeats some names (the first block is
/// formatted for display, the second is raw numbers), so `nth` picks the copy.
struct Cols {
    headers: Vec<String>,
}

impl Cols {
    fn find(&self, names: &[&str], nth: usize) -> Option<usize> {
        for n in names {
            let hits: Vec<usize> = self
                .headers
                .iter()
                .enumerate()
                .filter(|(_, h)| h.trim().eq_ignore_ascii_case(n))
                .map(|(i, _)| i)
                .collect();
            if let Some(i) = hits.get(nth).or_else(|| hits.last()) {
                return Some(*i);
            }
        }
        None
    }
}

/// One row of the Strava archive table.
#[derive(Debug, Clone)]
pub struct ArchiveRow {
    pub strava_id: i64,
    pub file: Option<String>,
    pub activity: Parsed,
}

fn num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

fn text(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn parse_strava_archive(text_in: &str) -> Result<Vec<ArchiveRow>, String> {
    let rows = parse_csv(text_in);
    let mut it = rows.into_iter();
    let headers = it.next().ok_or("activities.csv пустой")?;
    let c = Cols { headers };
    let col = |names: &[&str], nth: usize| c.find(names, nth);
    let id = col(&["ID физической активности", "Activity ID"], 0).ok_or("в activities.csv нет колонки ID")?;
    let date = col(&["Дата тренировки", "Activity Date"], 0).ok_or("в activities.csv нет колонки с датой")?;
    let name = col(&["Название тренировки", "Activity Name"], 0);
    let kind = col(&["Тип активности", "Activity Type"], 0);
    let desc = col(&["Описание физической активности", "Activity Description"], 0);
    let elapsed = col(&["Общее время", "Elapsed Time"], 1);
    let moving = col(&["Время в движении", "Moving Time"], 0);
    let dist = col(&["Дистанция", "Distance"], 1);
    let gain = col(&["Набор высоты", "Elevation Gain"], 0);
    let avg_hr = col(&["Средний пульс", "Average Heart Rate"], 0);
    let max_hr = col(&["Макс. пульс", "Max Heart Rate"], 1);
    let cad = col(&["Средний каденс", "Average Cadence"], 0);
    let pwr = col(&["Средняя мощность (Ватт)", "Average Watts"], 0);
    let cal = col(&["Калории", "Calories"], 0);
    let effort = col(&["Относительное усилие", "Relative Effort"], 1);
    let file = col(&["Название файла", "Filename"], 0);
    let gear = col(&["Снаряжение для физической активности", "Activity Gear"], 0);
    let steps = col(&["Всего шагов", "Total Steps"], 0);
    let sets = col(&["Всего подходов", "Total Sets"], 0);
    let reps = col(&["Общее количество повторений", "Total Reps"], 0);
    let lifted = col(&["Общий поднятый вес", "Total Weight Lifted"], 0);

    let mut out = Vec::new();
    for r in it {
        if r.len() <= id.max(date) {
            continue;
        }
        let get = |i: Option<usize>| i.and_then(|i| r.get(i)).map(|s| s.as_str()).unwrap_or("");
        let Ok(sid) = r[id].trim().parse::<i64>() else { continue };
        let Some(start) = parse_strava_csv_date(&r[date]) else { continue };
        let label = text(get(kind));
        let mut a = Parsed {
            sport: sport_from_strava(label.as_deref().unwrap_or("")).to_string(),
            sport_label: label,
            name: text(get(name)),
            description: text(get(desc)),
            start_utc: start,
            elapsed_s: num(get(elapsed)),
            moving_s: num(get(moving)),
            distance_m: num(get(dist)).filter(|d| *d > 0.0),
            elev_gain_m: num(get(gain)),
            avg_hr: num(get(avg_hr)),
            max_hr: num(get(max_hr)),
            avg_cadence: num(get(cad)),
            avg_power: num(get(pwr)),
            calories: num(get(cal)),
            relative_effort: num(get(effort)),
            gear: text(get(gear)),
            ..Default::default()
        };
        a.steps = num(get(steps)).map(|v| v as i64);
        a.total_sets = num(get(sets)).map(|v| v as i64);
        a.total_reps = num(get(reps)).map(|v| v as i64);
        a.weight_lifted_kg = num(get(lifted)).filter(|v| *v > 0.0);
        out.push(ArchiveRow { strava_id: sid, file: text(get(file)), activity: a });
    }
    Ok(out)
}
