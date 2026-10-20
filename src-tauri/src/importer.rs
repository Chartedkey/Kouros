//! Import of workout files: a Strava data archive (.zip with activities.csv),
//! loose .fit/.tcx/.gpx files (optionally .gz) or a zip of such files.
//! Preview first (nothing is written), then commit.

use crate::parse::{self, csv::ArchiveRow, time, Parsed};
use crate::store;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq)]
enum Status {
    New,
    /// Already in Kouros; the flag says whether it already has sample streams.
    Duplicate(bool),
}

struct Item {
    id: String,
    existing_id: Option<String>,
    strava_id: Option<i64>,
    parsed: Parsed,
    /// Original file to keep in files/activities (compressed bytes as found).
    original: Option<(String, Vec<u8>)>,
    status: Status,
}

pub struct Prepared {
    title: String,
    kind: &'static str,
    items: Vec<Item>,
    errors: Vec<(String, String)>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub start_utc: i64,
    pub sport: String,
    pub sport_label: Option<String>,
    pub name: Option<String>,
    pub distance_m: Option<f64>,
    pub elapsed_s: Option<f64>,
    pub avg_hr: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub name: String,
    pub why: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub title: String,
    pub kind: String,
    pub found: usize,
    pub existing: usize,
    pub new: usize,
    pub mergeable: usize,
    pub errors: Vec<ImportError>,
    pub new_rows: Vec<PreviewRow>,
    pub first_utc: Option<i64>,
    pub last_utc: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub merged: usize,
    pub skipped: usize,
    pub errors: usize,
}

fn lower_ext(name: &str) -> String {
    name.to_lowercase()
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| format!("не удалось распаковать: {e}"))?;
    Ok(out)
}

/// Parses one workout file by its name: .fit, .tcx, .gpx, each optionally .gz.
pub fn parse_file(name: &str, bytes: &[u8]) -> Result<Parsed, String> {
    let n = lower_ext(name);
    let (inner, data) = if let Some(base) = n.strip_suffix(".gz") {
        (base.to_string(), gunzip(bytes)?)
    } else {
        (n.clone(), bytes.to_vec())
    };
    if inner.ends_with(".fit") {
        parse::fit::parse(&data)
    } else if inner.ends_with(".tcx") {
        parse::xml::parse_tcx(String::from_utf8_lossy(&data).trim_start())
    } else if inner.ends_with(".gpx") {
        parse::xml::parse_gpx(String::from_utf8_lossy(&data).trim_start())
    } else {
        Err("формат не поддерживается — нужен .fit, .gpx, .tcx, .gz или .zip".into())
    }
}

fn is_workout_file(name: &str) -> bool {
    let n = lower_ext(name);
    let n = n.strip_suffix(".gz").unwrap_or(&n);
    n.ends_with(".fit") || n.ends_with(".tcx") || n.ends_with(".gpx")
}

fn base_name(p: &str) -> String {
    p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p).to_string()
}

/// Archive row (authoritative summary) + parsed file (streams, sets, device).
fn combine(row: &ArchiveRow, file: Option<Parsed>) -> Parsed {
    let mut a = row.activity.clone();
    if let Some(f) = file {
        a.elapsed_s = a.elapsed_s.or(f.elapsed_s);
        a.moving_s = a.moving_s.or(f.moving_s);
        a.distance_m = a.distance_m.or(f.distance_m);
        a.elev_gain_m = a.elev_gain_m.or(f.elev_gain_m);
        a.avg_hr = a.avg_hr.or(f.avg_hr);
        a.max_hr = a.max_hr.or(f.max_hr);
        a.avg_cadence = a.avg_cadence.or(f.avg_cadence);
        a.avg_power = a.avg_power.or(f.avg_power);
        a.calories = a.calories.or(f.calories);
        a.total_sets = a.total_sets.or(f.total_sets);
        a.total_reps = a.total_reps.or(f.total_reps);
        a.weight_lifted_kg = a.weight_lifted_kg.or(f.weight_lifted_kg);
        a.device = f.device;
        a.sets = f.sets;
        // Sample times are relative to the file's own start; shift if the CSV start differs.
        let shift = f.start_utc - a.start_utc;
        a.streams = f.streams;
        if shift != 0 {
            a.streams.t.iter_mut().for_each(|t| *t += shift);
        }
    }
    a
}

fn classify(conn: &Connection, strava_id: Option<i64>, p: &Parsed) -> (Status, Option<String>) {
    if let Some(sid) = strava_id {
        if let Some((id, has)) = store::exists_strava(conn, sid) {
            return (Status::Duplicate(has), Some(id));
        }
    }
    if let Some((id, has)) = store::find_by_time(conn, p.start_utc, p.elapsed_s) {
        return (Status::Duplicate(has), Some(id));
    }
    (Status::New, None)
}

pub fn prepare(conn: &Connection, paths: &[String]) -> Result<Prepared, String> {
    if paths.is_empty() {
        return Err("Файлы не выбраны".into());
    }
    let mut items: Vec<Item> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
    let mut kind: &'static str = "files";
    let title = if paths.len() == 1 { base_name(&paths[0]) } else { format!("{} файлов", paths.len()) };

    for path in paths {
        let name = base_name(path);
        if lower_ext(&name).ends_with(".zip") {
            let file = std::fs::File::open(path).map_err(|e| format!("{name}: {e}"))?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{name}: это не zip-архив ({e})"))?;
            // Index entries by full path and by base name.
            let mut by_path: HashMap<String, usize> = HashMap::new();
            let mut by_base: HashMap<String, usize> = HashMap::new();
            let mut csv_idx = None;
            for i in 0..zip.len() {
                let Ok(e) = zip.by_index(i) else { continue };
                let n = e.name().replace('\\', "/");
                if n.to_lowercase().ends_with("activities.csv") && !n.contains("__MACOSX") {
                    let depth = n.matches('/').count();
                    if csv_idx.map(|(_, d)| depth < d).unwrap_or(true) {
                        csv_idx = Some((i, depth));
                    }
                }
                by_base.insert(base_name(&n), i);
                by_path.insert(n, i);
            }
            let read = |zip: &mut zip::ZipArchive<std::fs::File>, i: usize| -> Result<Vec<u8>, String> {
                let mut e = zip.by_index(i).map_err(|e| e.to_string())?;
                let mut buf = Vec::new();
                e.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                Ok(buf)
            };
            if let Some((ci, _)) = csv_idx {
                kind = "strava_archive";
                let prefix = {
                    let e = zip.by_index(ci).map_err(|e| e.to_string())?;
                    let n = e.name().replace('\\', "/");
                    n[..n.len() - "activities.csv".len()].to_string()
                };
                let csv_bytes = read(&mut zip, ci)?;
                let rows = parse::csv::parse_strava_archive(&String::from_utf8_lossy(&csv_bytes))?;
                for row in rows {
                    let mut file_parsed = None;
                    let mut original = None;
                    if let Some(f) = &row.file {
                        let full = format!("{prefix}{f}");
                        let idx = by_path.get(&full).or_else(|| by_path.get(f)).or_else(|| by_base.get(&base_name(f)));
                        match idx {
                            Some(&i) => match read(&mut zip, i) {
                                Ok(bytes) => {
                                    match parse_file(f, &bytes) {
                                        Ok(p) => file_parsed = Some(p),
                                        Err(e) => errors.push((f.clone(), e)),
                                    }
                                    original = Some((base_name(f), bytes));
                                }
                                Err(e) => errors.push((f.clone(), e)),
                            },
                            None => errors.push((f.clone(), "файл не найден в архиве".into())),
                        }
                    }
                    let parsed = combine(&row, file_parsed);
                    let (status, existing_id) = classify(conn, Some(row.strava_id), &parsed);
                    items.push(Item {
                        id: store::activity_id_for(Some(row.strava_id), &parsed),
                        existing_id,
                        strava_id: Some(row.strava_id),
                        parsed,
                        original,
                        status,
                    });
                }
            } else {
                for i in 0..zip.len() {
                    let ename = match zip.by_index(i) {
                        Ok(e) if !e.is_dir() => e.name().to_string(),
                        _ => continue,
                    };
                    if !is_workout_file(&ename) {
                        continue;
                    }
                    let bytes = read(&mut zip, i)?;
                    push_loose(conn, &mut items, &mut errors, &ename, bytes);
                }
            }
        } else {
            match std::fs::read(path) {
                Ok(bytes) => push_loose(conn, &mut items, &mut errors, &name, bytes),
                Err(e) => errors.push((name, e.to_string())),
            }
        }
    }
    // The same workout can appear twice in one batch — keep the first.
    let mut seen = std::collections::HashSet::new();
    items.retain(|it| seen.insert(it.id.clone()));
    Ok(Prepared { title, kind, items, errors })
}

fn push_loose(conn: &Connection, items: &mut Vec<Item>, errors: &mut Vec<(String, String)>, name: &str, bytes: Vec<u8>) {
    match parse_file(name, &bytes) {
        Ok(mut p) => {
            if p.name.is_none() {
                p.name = Some(base_name(name).split('.').next().unwrap_or("").replace('_', " "));
            }
            let (status, existing_id) = classify(conn, None, &p);
            items.push(Item {
                id: store::activity_id_for(None, &p),
                existing_id,
                strava_id: None,
                parsed: p,
                original: Some((base_name(name), bytes)),
                status,
            });
        }
        Err(e) => errors.push((base_name(name), e)),
    }
}

pub fn preview(p: &Prepared) -> Preview {
    let new: Vec<&Item> = p.items.iter().filter(|i| i.status == Status::New).collect();
    let existing = p.items.len() - new.len();
    let mergeable = p
        .items
        .iter()
        .filter(|i| i.status == Status::Duplicate(false) && !i.parsed.streams.is_empty())
        .count();
    let mut rows: Vec<&&Item> = new.iter().collect();
    rows.sort_by(|a, b| b.parsed.start_utc.cmp(&a.parsed.start_utc));
    Preview {
        title: p.title.clone(),
        kind: p.kind.to_string(),
        found: p.items.len() + if p.kind == "files" { p.errors.len() } else { 0 },
        existing,
        new: new.len(),
        mergeable,
        errors: p.errors.iter().map(|(n, w)| ImportError { name: n.clone(), why: w.clone() }).collect(),
        new_rows: rows
            .iter()
            .take(8)
            .map(|i| PreviewRow {
                start_utc: i.parsed.start_utc,
                sport: i.parsed.sport.clone(),
                sport_label: i.parsed.sport_label.clone(),
                name: i.parsed.name.clone(),
                distance_m: i.parsed.distance_m,
                elapsed_s: i.parsed.elapsed_s,
                avg_hr: i.parsed.avg_hr,
            })
            .collect(),
        first_utc: p.items.iter().map(|i| i.parsed.start_utc).min(),
        last_utc: p.items.iter().map(|i| i.parsed.start_utc).max(),
    }
}

fn save_original(root: &Path, id: &str, original: &Option<(String, Vec<u8>)>) -> Option<String> {
    let (name, bytes) = original.as_ref()?;
    let lname = name.to_lowercase();
    let ext = if lname.ends_with(".gz") {
        let inner = &lname[..lname.len() - 3];
        format!("{}.gz", inner.rsplit('.').next().unwrap_or("bin"))
    } else {
        lname.rsplit('.').next().unwrap_or("bin").to_string()
    };
    let rel = format!("files/activities/{id}.{ext}");
    let full: PathBuf = root.join(&rel);
    std::fs::create_dir_all(full.parent()?).ok()?;
    std::fs::write(&full, bytes).ok()?;
    Some(rel)
}

pub fn commit(conn: &mut Connection, root: &Path, p: Prepared, merge: bool) -> Result<ImportResult, String> {
    let mut res = ImportResult { added: 0, merged: 0, skipped: 0, errors: p.errors.len() };
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let source = if p.kind == "strava_archive" { "strava_archive" } else { "file" };
    for it in &p.items {
        match it.status {
            Status::New => {
                let file_path = save_original(root, &it.id, &it.original);
                store::insert_activity(&tx, &it.id, it.strava_id, source, file_path.as_deref(), &it.parsed)?;
                res.added += 1;
            }
            Status::Duplicate(had) => {
                let target = it.existing_id.as_deref().unwrap_or(&it.id);
                if merge {
                    let file_path = if had { None } else { save_original(root, target, &it.original) };
                    if store::merge_activity(&tx, target, it.strava_id, file_path.as_deref(), &it.parsed, had)? {
                        res.merged += 1;
                    } else {
                        res.skipped += 1;
                    }
                } else {
                    res.skipped += 1;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    store::add_import_log(
        conn,
        &store::ImportLog {
            at: time::iso(store::now()),
            title: p.title.clone(),
            kind: p.kind.to_string(),
            found: p.items.len() as i64,
            added: res.added as i64,
            merged: res.merged as i64,
            skipped: res.skipped as i64,
            errors: res.errors as i64,
        },
    );
    if source == "strava_archive" {
        crate::db::log(conn, "import", &format!("Импорт архива Strava: +{}", res.added));
    }
    Ok(res)
}
