use ring::digest;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub path: String,
    pub original: String,
    pub size: u64,
    pub sha256: String,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn safe_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.len() > 2048 || name.contains(['\\', ':', '\0']) || !name.starts_with("files/") {
        return Err("Недопустимый путь файла".into());
    }
    let rel = Path::new(name);
    if rel.components().any(|c| !matches!(c, Component::Normal(_)))
        || name
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("Недопустимый путь файла".into());
    }
    let mut p = root.to_path_buf();
    for component in rel.components() {
        p.push(component);
        if p.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err("Ссылки на внешние файлы не передаются".into());
        }
    }
    Ok(p)
}
pub fn hash(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(err)?;
    let mut h = digest::Context::new(&digest::SHA256);
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b).map_err(err)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn manifest(
    root: &Path,
    cached: &serde_json::Value,
) -> Result<(Vec<FileInfo>, serde_json::Value), String> {
    let mut files = Vec::new();
    let mut cache = serde_json::Map::new();
    let base = root.join("files");
    if !base.exists() {
        return Ok((files, serde_json::Value::Object(cache)));
    }
    let mut dirs = vec![base];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).map_err(err)? {
            let e = entry.map_err(err)?;
            let meta = e.path().symlink_metadata().map_err(err)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                dirs.push(e.path());
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let name = e
                .path()
                .strip_prefix(root)
                .map_err(err)?
                .to_string_lossy()
                .replace('\\', "/");
            let path = safe_path(root, &name)?;
            let stamp = format!(
                "{}:{}",
                meta.len(),
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let sha = if cached[&name]["stamp"] == stamp {
                match cached[&name]["sha"].as_str() {
                    Some(s) => s.to_string(),
                    None => hash(&path)?,
                }
            } else {
                hash(&path)?
            };
            cache.insert(name.clone(), serde_json::json!({"stamp":stamp,"sha":sha}));
            let canonical = format!(
                "files/peer/{}/{}",
                sha,
                path.file_name().ok_or("Нет имени файла")?.to_string_lossy()
            );
            files.push(FileInfo {
                path: canonical,
                original: name,
                size: meta.len(),
                sha256: sha,
            });
            if files.len() > 50000 {
                return Err("Слишком много вложений для одной синхронизации".into());
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok((files, serde_json::Value::Object(cache)))
}
pub fn write_chunk(
    root: &Path,
    info: &FileInfo,
    offset: u64,
    data: &[u8],
    finish: bool,
) -> Result<(), String> {
    if info.sha256.len() != 64
        || !info.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || info.size > 2 * 1024 * 1024 * 1024
        || data.len() > 1024 * 1024
    {
        return Err("Недопустимое вложение".into());
    }
    if !info
        .path
        .starts_with(&format!("files/peer/{}/", info.sha256))
    {
        return Err("Неверный адрес вложения".into());
    }
    let dest = safe_path(root, &info.path)?;
    let folder = root.join("peer-transfer");
    std::fs::create_dir_all(&folder).map_err(err)?;
    let tmp = folder.join(format!("{}.part", info.sha256));
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true);
    if offset == 0 {
        opts.truncate(true);
    } else {
        opts.append(true);
    }
    let mut out = opts.open(&tmp).map_err(err)?;
    if out.metadata().map_err(err)?.len() != offset {
        return Err("Повторите передачу вложения".into());
    }
    out.write_all(data).map_err(err)?;
    if offset + data.len() as u64 > info.size {
        return Err("Вложение больше заявленного".into());
    }
    if finish {
        out.sync_all().map_err(err)?;
        drop(out);
        if tmp.metadata().map_err(err)?.len() != info.size || hash(&tmp)? != info.sha256 {
            let _ = std::fs::remove_file(&tmp);
            return Err("Контрольная сумма вложения не совпала".into());
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        if dest.exists() {
            if hash(&dest)? == info.sha256 {
                std::fs::remove_file(tmp).map_err(err)?;
                return Ok(());
            }
            let conflicts = root.join("peer-file-conflicts");
            std::fs::create_dir_all(&conflicts).map_err(err)?;
            let old = hash(&dest)?;
            let backup = conflicts.join(old);
            std::fs::copy(&dest, backup).map_err(err)?;
            std::fs::remove_file(&dest).map_err(err)?;
        }
        std::fs::rename(tmp, dest).map_err(err)?;
    }
    Ok(())
}
pub fn wire_paths(
    ch: &mut crate::peer_data::Change,
    root: &Path,
    receiving: bool,
) -> Result<(), String> {
    let field = match ch.t.as_str() {
        "activities" => "file_path",
        "lab_reports" => "file",
        _ => return Ok(()),
    };
    if let Some(row) = ch.row.as_mut().and_then(serde_json::Value::as_object_mut) {
        if let Some(name) = row.get(field).and_then(|x| x.as_str()).map(str::to_string) {
            let value = if receiving {
                if let Some(rel) = name.strip_prefix("@file:") {
                    safe_path(root, rel)?;
                    Some(if ch.t == "lab_reports" {
                        rel.strip_prefix("files/")
                            .ok_or("Неверный путь документа")?
                            .to_string()
                    } else {
                        rel.to_string()
                    })
                } else {
                    return Err("Неверный путь в записи".into());
                }
            } else {
                let normalized = name.replace('\\', "/");
                let path = Path::new(&normalized);
                let relative = if path.is_absolute() {
                    path.strip_prefix(root)
                        .ok()
                        .map(|r| r.to_string_lossy().replace('\\', "/"))
                } else if normalized.starts_with("files/") {
                    Some(normalized)
                } else if ch.t == "lab_reports" {
                    Some(format!("files/{normalized}"))
                } else {
                    None
                };
                if let Some(r) = relative.filter(|r| safe_path(root, r).is_ok()) {
                    let p = safe_path(root, &r)?;
                    if p.is_file() {
                        let sha = hash(&p)?;
                        let name = p.file_name().ok_or("Нет имени вложения")?.to_string_lossy();
                        Some(format!("@file:files/peer/{sha}/{name}"))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            row.insert(
                field.into(),
                value
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }
    Ok(())
}

pub fn source_path(root: &Path, info: &FileInfo) -> Result<PathBuf, String> {
    let dest = safe_path(root, &info.path)?;
    if dest.is_file() {
        Ok(dest)
    } else {
        safe_path(root, &info.original)
    }
}

fn reference_path(root: &Path, value: &str) -> Result<String, String> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .map_err(|_| "Вложение вне папки Kouros")?
            .to_string_lossy()
            .replace('\\', "/")
    } else if normalized.starts_with("files/") {
        normalized
    } else if normalized.starts_with("peer/") {
        format!("files/{normalized}")
    } else {
        return Err("Вложение вне папки Kouros".into());
    };
    Ok(relative)
}
fn verify_reference(root: &Path, value: &str) -> Result<(), String> {
    let relative = reference_path(root, value)?;
    let parts: Vec<_> = relative.split('/').collect();
    if parts.len() != 4
        || parts[0] != "files"
        || parts[1] != "peer"
        || parts[2].len() != 64
        || !parts[2].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Неверная ссылка на вложение".into());
    }
    let file = safe_path(root, &relative)?;
    if !file.is_file() || hash(&file)? != parts[2] {
        return Err("Не все вложения получены. Повторите синхронизацию".into());
    }
    Ok(())
}
pub fn verify_changes(root: &Path, changes: &[crate::peer_data::Change]) -> Result<(), String> {
    for change in changes {
        let field = match change.t.as_str() {
            "activities" => "file_path",
            "lab_reports" => "file",
            _ => continue,
        };
        if let Some(value) = change.row.as_ref().and_then(|r| r[field].as_str()) {
            verify_reference(root, value)?;
        }
    }
    Ok(())
}
pub fn verify_stage(root: &Path, c: &rusqlite::Connection) -> Result<(), String> {
    let mut stmt=c.prepare("SELECT CASE WHEN json_extract(body,'$.t')='activities' THEN json_extract(body,'$.row.file_path') ELSE json_extract(body,'$.row.file')END FROM peer_stage WHERE json_extract(body,'$.t') IN('activities','lab_reports')").map_err(err)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, Option<String>>(0))
        .map_err(err)?;
    for row in rows {
        if let Some(path) = row.map_err(err)? {
            verify_reference(root, &path)?;
        }
    }
    Ok(())
}

/// Materialize an immutable copy if this device already owns the same bytes under
/// an original import path. A manifest hash match alone is not a usable file path.
pub fn materialize_refs(
    root: &Path,
    changes: &[crate::peer_data::Change],
    cache: &serde_json::Value,
) -> Result<(), String> {
    for change in changes {
        let field = match change.t.as_str() {
            "activities" => "file_path",
            "lab_reports" => "file",
            _ => continue,
        };
        let Some(value) = change.row.as_ref().and_then(|r| r[field].as_str()) else {
            continue;
        };
        if verify_reference(root, value).is_ok() {
            continue;
        }
        let relative = reference_path(root, value)?;
        let parts: Vec<_> = relative.split('/').collect();
        if parts.len() != 4
            || parts[0] != "files"
            || parts[1] != "peer"
            || parts[2].len() != 64
            || !parts[2].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Неверная ссылка на вложение".into());
        }
        let expected = parts[2].to_string();
        let aliases = cache
            .as_object()
            .ok_or("Не все вложения получены. Повторите синхронизацию")?;
        let mut source = None;
        for (name, info) in aliases {
            if info["sha"].as_str() == Some(expected.as_str()) {
                let file = safe_path(root, name)?;
                if file.is_file() && hash(&file)? == expected {
                    source = Some((name.clone(), file));
                    break;
                }
            }
        }
        let (original, file) = source.ok_or("Не все вложения получены. Повторите синхронизацию")?;
        let size = file.metadata().map_err(err)?.len();
        let info = FileInfo {
            path: relative,
            original,
            size,
            sha256: expected,
        };
        let mut input = std::fs::File::open(file).map_err(err)?;
        let mut offset = 0;
        loop {
            let mut block = vec![0; 1024 * 1024];
            let n = input.read(&mut block).map_err(err)?;
            block.truncate(n);
            let done = offset + n as u64 == size;
            write_chunk(root, &info, offset, &block, done)?;
            offset += n as u64;
            if done {
                break;
            }
            if n == 0 {
                return Err("Вложение изменилось во время копирования".into());
            }
        }
    }
    Ok(())
}
