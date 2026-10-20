//! Export of workouts into exports/: a CSV summary, GPX tracks, or the original
//! files as they came from the watch (FIT/TCX/GPX).

use crate::parse::time;
use crate::store::{self, ActivityRow};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub folder: String,
    pub files: usize,
    pub skipped: usize,
}

fn sport_ru(s: &str) -> &'static str {
    match s {
        "run" => "Бег",
        "walk" => "Ходьба",
        "hike" => "Поход",
        "ride" => "Велосипед",
        "ride_virtual" => "Виртуальный заезд",
        "strength" => "Силовая",
        "workout" => "Тренировка",
        "yoga" => "Йога",
        "swim" => "Плавание",
        _ => "Другое",
    }
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt(v: Option<f64>, dec: usize) -> String {
    v.map(|x| format!("{x:.dec$}")).unwrap_or_default()
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(40)
        .collect()
}

fn gpx(a: &ActivityRow, streams: &str) -> Option<String> {
    let v: Value = serde_json::from_str(streams).ok()?;
    let t = v["t"].as_array()?;
    let lat = v["lat"].as_array()?;
    let lng = v["lng"].as_array()?;
    let alt = v["alt"].as_array();
    let hr = v["hr"].as_array();
    let cad = v["cad"].as_array();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<gpx version=\"1.1\" creator=\"Kouros\" xmlns=\"http://www.topografix.com/GPX/1/1\" xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\">\n");
    let name = a.name.clone().unwrap_or_default().replace('&', "&amp;").replace('<', "&lt;");
    let kind = match a.sport.as_str() {
        "run" => "running",
        "walk" => "walking",
        "hike" => "hiking",
        "ride" | "ride_virtual" => "cycling",
        _ => "other",
    };
    out.push_str(&format!(" <trk><name>{name}</name><type>{kind}</type><trkseg>\n"));
    let mut n = 0;
    for (i, tv) in t.iter().enumerate() {
        let (Some(la), Some(lo)) = (lat.get(i).and_then(|x| x.as_f64()), lng.get(i).and_then(|x| x.as_f64())) else {
            continue;
        };
        let ts = a.start_utc + tv.as_i64().unwrap_or(0);
        out.push_str(&format!("  <trkpt lat=\"{la:.6}\" lon=\"{lo:.6}\">"));
        if let Some(e) = alt.and_then(|x| x.get(i)).and_then(|x| x.as_f64()) {
            out.push_str(&format!("<ele>{e:.1}</ele>"));
        }
        out.push_str(&format!("<time>{}</time>", time::iso(ts)));
        let h = hr.and_then(|x| x.get(i)).and_then(|x| x.as_f64());
        let c = cad.and_then(|x| x.get(i)).and_then(|x| x.as_f64());
        if h.is_some() || c.is_some() {
            out.push_str("<extensions><gpxtpx:TrackPointExtension>");
            if let Some(h) = h {
                out.push_str(&format!("<gpxtpx:hr>{h:.0}</gpxtpx:hr>"));
            }
            if let Some(c) = c {
                out.push_str(&format!("<gpxtpx:cad>{c:.0}</gpxtpx:cad>"));
            }
            out.push_str("</gpxtpx:TrackPointExtension></extensions>");
        }
        out.push_str("</trkpt>\n");
        n += 1;
    }
    out.push_str(" </trkseg></trk>\n</gpx>\n");
    if n == 0 {
        None
    } else {
        Some(out)
    }
}

/// `format`: "csv" | "gpx" | "original". `days`: None = everything.
pub fn export(conn: &Connection, root: &Path, days: Option<i64>, format: &str) -> Result<ExportResult, String> {
    let to = store::now() + 86400;
    let from = days.map(|d| store::now() - d * 86400).unwrap_or(0);
    let rows = store::activities_between(conn, from, to)?;
    if rows.is_empty() {
        return Err("За этот период тренировок нет".into());
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M").to_string();
    let folder = root.join("exports").join(format!("kouros-{format}-{stamp}"));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut files = 0;
    let mut skipped = 0;
    match format {
        "csv" => {
            let mut s = String::from("\u{feff}Дата (UTC),Тип,Название,Дистанция (км),Время (мин),В движении (мин),Набор высоты (м),Средний пульс,Макс. пульс,Калории,Относительное усилие,Подходы,Повторения,Устройство,Источник,ID Strava\n");
            for a in &rows {
                let cells = [
                    time::iso(a.start_utc).replace('T', " ").replace('Z', ""),
                    sport_ru(&a.sport).to_string(),
                    csv_cell(a.name.as_deref().unwrap_or("")),
                    opt(a.distance_m.map(|d| d / 1000.0), 2),
                    opt(a.elapsed_s.map(|x| x / 60.0), 1),
                    opt(a.moving_s.map(|x| x / 60.0), 1),
                    opt(a.elev_gain_m, 0),
                    opt(a.avg_hr, 0),
                    opt(a.max_hr, 0),
                    opt(a.calories, 0),
                    opt(a.relative_effort, 0),
                    a.total_sets.map(|x| x.to_string()).unwrap_or_default(),
                    a.total_reps.map(|x| x.to_string()).unwrap_or_default(),
                    csv_cell(a.device.as_deref().unwrap_or("")),
                    a.source.clone(),
                    a.strava_id.map(|x| x.to_string()).unwrap_or_default(),
                ];
                s.push_str(&cells.join(","));
                s.push('\n');
            }
            std::fs::write(folder.join("kouros_trainings.csv"), s).map_err(|e| e.to_string())?;
            files = 1;
        }
        "gpx" => {
            for a in &rows {
                let Some(js) = (if a.has_gps { store::streams_json(conn, &a.id) } else { None }) else {
                    skipped += 1;
                    continue;
                };
                match gpx(a, &js) {
                    Some(text) => {
                        let name = format!(
                            "{}_{}_{}.gpx",
                            time::ymd(a.start_utc, 3 * 3600),
                            a.sport,
                            safe_name(a.name.as_deref().unwrap_or(""))
                        );
                        std::fs::write(folder.join(name), text).map_err(|e| e.to_string())?;
                        files += 1;
                    }
                    None => skipped += 1,
                }
            }
        }
        _ => {
            for a in &rows {
                let Some(rel) = &a.file_path else {
                    skipped += 1;
                    continue;
                };
                let src = root.join(rel);
                let ext = rel.split_once('.').map(|(_, e)| e).unwrap_or("bin");
                let name = format!("{}_{}_{}.{ext}", time::ymd(a.start_utc, 3 * 3600), a.sport, a.id);
                if std::fs::copy(&src, folder.join(name)).is_ok() {
                    files += 1;
                } else {
                    skipped += 1;
                }
            }
        }
    }
    Ok(ExportResult { folder: folder.to_string_lossy().to_string(), files, skipped })
}
