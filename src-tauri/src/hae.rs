//! Health Auto Export (1.3): Apple Health data straight from the athlete's iPhone, without freddy.
//!
//! Three ways in, one importer:
//! - the phone sends JSON to a small receiver on the home network (`http://<pc>:<port>/hae?key=…`);
//!   the body is saved to `Kouros\inbox` first and read from there, so nothing is lost when the
//!   app is locked;
//! - a folder (Google Drive, Dropbox, iCloud for Windows) where the app saves its files;
//! - a file chosen by hand (history: ZIP or JSON).
//!
//! Rows are stored with provider `HAE` under Apple's metric names; the view `hm` shows them only
//! for the data groups switched to Health Auto Export (`sources.rs`).

use crate::db;
use crate::parse::hae::{self as p, Batch, Seg};
use crate::store::{self, MetricRow};
use crate::AppState;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const HAE: &str = "HAE";
pub const DEFAULT_PORT: u16 = 8765;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- settings

pub fn cfg(conn: &Connection) -> Value {
    db::get_setting(conn, "hae").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}

fn save_cfg(conn: &Connection, v: &Value) -> Result<(), String> {
    db::set_setting(conn, "hae", &v.to_string())
}

fn new_token() -> String {
    let mut b = [0u8; 12];
    let _ = getrandom::getrandom(&mut b);
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    b.iter().map(|x| A[(*x as usize) % A.len()] as char).collect()
}

/// The computer's address on the home network (the interface used to reach the router).
pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // No packet is sent: connect() on UDP only picks the route.
    s.connect("192.168.1.1:80").or_else(|_| s.connect("10.0.0.1:80")).ok()?;
    let ip = s.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() { None } else { Some(ip.to_string()) }
}

fn root(app: &AppHandle) -> Option<PathBuf> {
    app.path().home_dir().ok().map(|h| h.join("Kouros"))
}

fn inbox(app: &AppHandle) -> Option<PathBuf> {
    let d = root(app)?.join("inbox");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&mut Connection) -> Result<T, String>) -> Result<T, String> {
    let st = app.state::<AppState>();
    if *st.unlocked.lock().unwrap() != Some(true) {
        return Err("locked".into());
    }
    let mut lock = st.db.lock().unwrap();
    let conn = lock.as_mut().ok_or("locked")?;
    f(conn)
}

/// Live state of the receiver (not stored).
#[derive(Default)]
struct Live {
    port: u16,
    running: bool,
    error: Option<String>,
    last_push: Option<i64>,
    last_check: Option<i64>,
    progress: Option<Value>,
}

fn live() -> &'static Mutex<Live> {
    static L: std::sync::OnceLock<Mutex<Live>> = std::sync::OnceLock::new();
    L.get_or_init(|| Mutex::new(Live::default()))
}

static STOP: AtomicU64 = AtomicU64::new(0);
static DRAINING: AtomicBool = AtomicBool::new(false);
static SEQ: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------- import

#[derive(Default, Clone)]
pub struct Stats {
    pub samples: usize,
    pub first: Option<String>,
    pub last: Option<String>,
    pub unknown: BTreeMap<String, usize>,
    pub metrics: usize,
}

impl Stats {
    fn json(&self) -> Value {
        json!({"samples": self.samples, "first": self.first, "last": self.last, "metrics": self.metrics,
               "unknown": self.unknown.keys().collect::<Vec<_>>()})
    }
}

fn row(o: p::Out) -> MetricRow {
    MetricRow {
        metric: o.metric,
        date: o.date,
        ts: o.ts,
        provider: HAE.into(),
        source: Some(o.device.clone()),
        device: o.device,
        source_id: o.source_id,
        num: o.num,
        txt: o.txt,
        unit: if o.unit.is_empty() { None } else { Some(o.unit) },
    }
}

/// Writes one metric's batch: finished rows, then bundles merged with what is stored and their
/// daily figures.
fn write_batch(conn: &mut Connection, b: Batch) -> Result<(), String> {
    let rows: Vec<MetricRow> = b.rows.into_iter().map(row).collect();
    if !rows.is_empty() {
        store::upsert_metrics(conn, &rows)?;
    }
    if b.bundles.is_empty() {
        return Ok(());
    }
    let mut touched: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out: Vec<MetricRow> = Vec::new();
    for bd in &b.bundles {
        let old: Option<String> = conn
            .query_row(
                "SELECT txt FROM health_metrics WHERE metric = ?1 AND date = ?2 AND provider = ?3 AND device = ?4 AND source_id = '' AND ts = ?2",
                params![bd.metric, bd.date, HAE, bd.device],
                |r| r.get(0),
            )
            .ok();
        let old_pts = old.map(|t| p::points_from_txt(&t)).unwrap_or_default();
        let merged = p::merge_points(old_pts, &bd.points);
        out.push(MetricRow {
            metric: bd.metric.clone(),
            date: bd.date.clone(),
            ts: bd.date.clone(),
            provider: HAE.into(),
            device: bd.device.clone(),
            source_id: String::new(),
            source: Some(bd.device.clone()),
            num: None,
            txt: Some(p::points_json(&merged)),
            unit: Some(bd.unit.clone()),
        });
        touched.insert((bd.metric.clone(), bd.date.clone()));
    }
    store::upsert_metrics(conn, &out)?;
    let mut daily: Vec<MetricRow> = Vec::new();
    for (metric, date) in touched {
        if p::derive_rule(&metric).is_none() {
            continue;
        }
        let devs: Vec<(String, Vec<(i64, f64)>)> = {
            let mut st = conn
                .prepare("SELECT device, txt FROM health_metrics WHERE metric = ?1 AND date = ?2 AND provider = ?3 AND source_id = '' AND ts = ?2 AND txt IS NOT NULL")
                .map_err(err)?;
            let v: Vec<(String, String)> = st.query_map(params![metric, date, HAE], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.flatten().collect();
            v.into_iter().map(|(d, t)| (d, p::points_from_txt(&t))).collect()
        };
        if let Some(o) = p::derive(&metric, &date, &devs) {
            daily.push(row(o));
        }
    }
    if !daily.is_empty() {
        store::upsert_metrics(conn, &daily)?;
    }
    Ok(())
}

/// Sleep stages merged with stored nights of the same days, then cut into nights again.
fn write_sleep(conn: &mut Connection, segs: Vec<Seg>) -> Result<(), String> {
    if segs.is_empty() {
        return Ok(());
    }
    let mut days: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for s in &segs {
        for t in [s.a - 86400, s.a, s.b, s.b + 86400] {
            days.entry(s.device.clone()).or_default().insert(crate::parse::time::ymd(t, s.off));
        }
    }
    let mut all = segs;
    let tx = conn.transaction().map_err(err)?;
    for (dev, ds) in &days {
        for d in ds {
            let old: Vec<String> = {
                let mut st = tx
                    .prepare("SELECT txt FROM health_metrics WHERE metric = 'sleep_analysis_raw' AND provider = ?1 AND device = ?2 AND date = ?3 AND txt IS NOT NULL")
                    .map_err(err)?;
                let v: Vec<String> = st.query_map(params![HAE, dev, d], |r| r.get(0)).map_err(err)?.flatten().collect();
                v
            };
            for t in old {
                all.extend(p::segs_from_txt(dev, &t));
            }
            tx.execute(
                "DELETE FROM health_metrics WHERE provider = ?1 AND device = ?2 AND date = ?3 AND metric IN
                 ('sleep_analysis_raw', 'sleep_analysis_core_seconds', 'sleep_analysis_deep_seconds', 'sleep_analysis_rem_seconds', 'sleep_analysis_awake_seconds')",
                params![HAE, dev, d],
            )
            .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    let rows: Vec<MetricRow> = p::sleep_rows(all).into_iter().map(row).collect();
    store::upsert_metrics(conn, &rows)?;
    Ok(())
}

/// Daily food and drink totals of the given days, summed from the stored entries.
fn write_food(conn: &mut Connection, days: &BTreeSet<String>) -> Result<(), String> {
    let tx = conn.transaction().map_err(err)?;
    for d in days {
        tx.execute(
            "DELETE FROM health_metrics WHERE provider = ?1 AND date = ?2 AND device = 'Merged' AND source_id = ''
             AND (metric LIKE 'dietary\\_%\\_total' ESCAPE '\\' OR metric = 'alcoholic_beverages_total')",
            params![HAE, d],
        )
        .map_err(err)?;
        tx.execute(
            "INSERT OR REPLACE INTO health_metrics (metric, date, ts, provider, device, source_id, source, num, txt, unit)
             SELECT CASE metric WHEN 'drink_entry_volume' THEN 'dietary_water_total'
                                WHEN 'drink_entry_alcohol_units' THEN 'alcoholic_beverages_total'
                                ELSE 'dietary_' || substr(metric, 12) || '_total' END,
                    date, date, ?1, 'Merged', '', 'Merged', sum(num), NULL, max(unit)
             FROM health_metrics
             WHERE provider = ?1 AND date = ?2 AND num IS NOT NULL
               AND ((metric LIKE 'food\\_entry\\_%' ESCAPE '\\' AND metric <> 'food_entry_name') OR metric IN ('drink_entry_volume', 'drink_entry_alcohol_units'))
             GROUP BY metric",
            params![HAE, d],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

/// Reads a whole export. `with` gives the database for a moment (each metric separately, so the
/// app stays responsive during a long history import).
fn import_reader<R: Read>(app: &AppHandle, r: R, label: &str) -> Result<Stats, String> {
    let mut stats = Stats::default();
    let mut segs: Vec<Seg> = Vec::new();
    let mut food: BTreeSet<String> = BTreeSet::new();
    p::read_all(r, &mut |mut b: Batch| {
        stats.samples += b.samples;
        if b.samples > 0 {
            stats.metrics += 1;
        }
        for (k, v) in std::mem::take(&mut b.unknown) {
            *stats.unknown.entry(k).or_insert(0) += v;
        }
        if let Some(f) = &b.first {
            if stats.first.as_deref().map(|x| f.as_str() < x).unwrap_or(true) {
                stats.first = Some(f.clone());
            }
        }
        if let Some(l) = &b.last {
            if stats.last.as_deref().map(|x| l.as_str() > x).unwrap_or(true) {
                stats.last = Some(l.clone());
            }
        }
        segs.append(&mut b.segs);
        food.extend(std::mem::take(&mut b.food_days));
        let prog = json!({"label": label, "samples": stats.samples, "first": stats.first, "last": stats.last});
        live().lock().unwrap().progress = Some(prog.clone());
        let _ = app.emit("hae-progress", prog);
        with_conn(app, |c| write_batch(c, b))
    })?;
    with_conn(app, |c| {
        write_sleep(c, std::mem::take(&mut segs))?;
        write_food(c, &food)?;
        crate::sources::install_view(c)?;
        let mut v = cfg(c);
        v["lastImport"] = json!({"at": store::now(), "how": label, "stats": stats.json()});
        save_cfg(c, &v)
    })?;
    live().lock().unwrap().progress = None;
    let _ = app.emit("data-changed", "hae");
    Ok(stats)
}

/// A .json, .json.gz or .zip file (every JSON inside a ZIP; GPX routes are skipped).
pub fn import_path(app: &AppHandle, path: &Path, label: &str) -> Result<Stats, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut f = std::fs::File::open(path).map_err(|e| format!("Не открыть файл: {e}"))?;
    let mut head = [0u8; 4];
    let n = f.read(&mut head).map_err(err)?;
    drop(f);
    let f = std::fs::File::open(path).map_err(err)?;
    if n >= 2 && head[0] == b'P' && head[1] == b'K' {
        let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("{name}: это не zip-архив ({e})"))?;
        let mut total = Stats::default();
        let mut found = false;
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(err)?;
            let en = entry.name().to_lowercase();
            if !en.ends_with(".json") || en.starts_with("__macosx") {
                continue;
            }
            found = true;
            let s = import_reader(app, entry, label)?;
            total.samples += s.samples;
            total.metrics += s.metrics;
            total.first = [total.first.clone(), s.first].into_iter().flatten().min();
            total.last = [total.last.clone(), s.last].into_iter().flatten().max();
            total.unknown.extend(s.unknown);
        }
        if !found {
            return Err("В архиве нет JSON-файла. В Health Auto Export выбери формат JSON".into());
        }
        return Ok(total);
    }
    if n >= 2 && head[0] == 0x1f && head[1] == 0x8b {
        return import_reader(app, flate2::read::GzDecoder::new(f), label);
    }
    if name.ends_with(".csv") {
        return Err("Это CSV. Kouros читает выгрузку в формате JSON — выбери его в Health Auto Export".into());
    }
    import_reader(app, f, label)
}

/// Imports everything waiting in the inbox (pushes saved while the app was busy or locked).
pub fn drain(app: &AppHandle) {
    if DRAINING.swap(true, Ordering::SeqCst) {
        return;
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            DRAINING.store(false, Ordering::SeqCst);
        }
    }
    let _done = Done;
    let Some(dir) = inbox(app) else { return };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().map(|e| e == "json").unwrap_or(false)).collect())
        .unwrap_or_default();
    files.sort();
    for f in files {
        match import_path(app, &f, "push") {
            Ok(_) => {
                let _ = std::fs::remove_file(&f);
            }
            Err(e) if e == "locked" => return,
            Err(e) => {
                let bad = dir.join("bad");
                let _ = std::fs::create_dir_all(&bad);
                let _ = std::fs::rename(&f, bad.join(f.file_name().unwrap_or_default()));
                live().lock().unwrap().error = Some(format!("Не удалось прочитать присланные данные: {e}"));
                let _ = with_conn(app, |c| {
                    db::log(c, "error", &format!("Health Auto Export: {e}"));
                    Ok(())
                });
            }
        }
    }
}

/// New or changed files in the chosen folder.
pub fn scan_folder(app: &AppHandle) -> Result<usize, String> {
    let (folder, seen) = with_conn(app, |c| {
        let v = cfg(c);
        Ok((v["folder"].as_str().unwrap_or("").to_string(), v["seen"].clone()))
    })?;
    if folder.is_empty() {
        return Ok(0);
    }
    let dir = PathBuf::from(&folder);
    if !dir.is_dir() {
        return Err(format!("Папка не найдена: {folder}"));
    }
    let mut seen: BTreeMap<String, String> = serde_json::from_value(seen).unwrap_or_default();
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut walk = vec![(dir.clone(), 0)];
    while let Some((d, depth)) = walk.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let pth = e.path();
            if pth.is_dir() {
                if depth < 2 {
                    walk.push((pth, depth + 1));
                }
                continue;
            }
            let n = pth.file_name().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !(n.ends_with(".json") || n.ends_with(".zip")) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let mt = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let sig = format!("{}:{}", m.len(), mt);
            let key = pth.to_string_lossy().to_string();
            // Skip files still being written (changed in the last 20 s).
            if (store::now() as u64).saturating_sub(mt) < 20 {
                continue;
            }
            if seen.get(&key) != Some(&sig) {
                files.push((pth, sig));
            }
        }
    }
    files.sort();
    let mut done = 0;
    for (f, sig) in files {
        let res = import_path(app, &f, "folder");
        if let Err(e) = &res {
            if e == "locked" {
                break;
            }
            live().lock().unwrap().error = Some(format!("{}: {e}", f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()));
        }
        seen.insert(f.to_string_lossy().to_string(), sig);
        done += 1;
        let s = serde_json::to_value(&seen).unwrap_or(Value::Null);
        let _ = with_conn(app, |c| {
            let mut v = cfg(c);
            v["seen"] = s;
            save_cfg(c, &v)
        });
    }
    Ok(done)
}

// ---------------------------------------------------------------- receiver

struct Req {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|x| u8::from_str_radix(x, 16).ok());
                match h {
                    Some(x) => {
                        out.push(x);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            x => {
                out.push(x);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_head(r: &mut BufReader<TcpStream>) -> Result<Req, String> {
    let mut line = String::new();
    r.read_line(&mut line).map_err(err)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, q) = target.split_once('?').unwrap_or((&target, ""));
    let query = q.split('&').filter(|x| !x.is_empty()).map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (url_decode(k), url_decode(v))
    }).collect();
    let mut headers = BTreeMap::new();
    let mut total = 0;
    loop {
        let mut h = String::new();
        let n = r.read_line(&mut h).map_err(err)?;
        total += n;
        if n == 0 || h == "\r\n" || h == "\n" || total > 64 * 1024 {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    Ok(Req { method, path: path.to_string(), query, headers })
}

fn respond(s: &mut TcpStream, code: &str, ctype: &str, body: &str) {
    let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: {ctype}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}", body.len());
    let _ = s.flush();
}

fn page(title: &str, text: &str, ok: bool) -> String {
    let c = if ok { "#3DDC97" } else { "#FF6B6B" };
    format!(
        "<!doctype html><html lang=\"ru\"><head><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Kouros</title></head>\
         <body style=\"margin:0;background:#0b0d10;color:#e8ecf1;font-family:-apple-system,system-ui,sans-serif;display:flex;min-height:100vh;align-items:center;justify-content:center\">\
         <div style=\"max-width:420px;padding:28px;text-align:center\"><div style=\"width:72px;height:72px;margin:0 auto 18px;border-radius:50%;border:6px solid {c};border-right-color:transparent\"></div>\
         <h1 style=\"font-size:22px;margin:0 0 10px\">{title}</h1><p style=\"color:#9aa4b2;line-height:1.5;margin:0\">{text}</p></div></body></html>"
    )
}

/// Copies the request body (Content-Length or chunked) into `w`.
fn copy_body(r: &mut BufReader<TcpStream>, h: &BTreeMap<String, String>, w: &mut impl Write) -> Result<u64, String> {
    let mut total = 0u64;
    let mut buf = vec![0u8; 1 << 16];
    if h.get("transfer-encoding").map(|v| v.to_lowercase().contains("chunked")).unwrap_or(false) {
        loop {
            let mut line = String::new();
            r.read_line(&mut line).map_err(err)?;
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or("0"), 16).map_err(|_| "Неверный блок данных".to_string())?;
            if size == 0 {
                let mut end = String::new();
                let _ = r.read_line(&mut end);
                break;
            }
            let mut left = size;
            while left > 0 {
                let k = r.read(&mut buf[..left.min(1 << 16)]).map_err(err)?;
                if k == 0 {
                    return Err("Соединение оборвалось".into());
                }
                w.write_all(&buf[..k]).map_err(err)?;
                left -= k;
                total += k as u64;
            }
            let mut crlf = String::new();
            r.read_line(&mut crlf).map_err(err)?;
        }
    } else {
        let mut left: u64 = h.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        while left > 0 {
            let k = r.read(&mut buf[..(left.min(1 << 16)) as usize]).map_err(err)?;
            if k == 0 {
                return Err("Соединение оборвалось".into());
            }
            w.write_all(&buf[..k]).map_err(err)?;
            left -= k as u64;
            total += k as u64;
        }
    }
    Ok(total)
}

fn handle(app: AppHandle, s: TcpStream, token: String) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(120)));
    let Ok(mut out) = s.try_clone() else { return };
    let mut r = BufReader::with_capacity(1 << 16, s);
    let Ok(req) = read_head(&mut r) else { return };
    let given = req.query.get("key").cloned().or_else(|| {
        req.headers.get("authorization").or(req.headers.get("x-api-key")).map(|v| v.trim_start_matches("Bearer ").trim_start_matches("bearer ").trim().to_string())
    });
    let ok_key = given.as_deref() == Some(token.as_str());
    let path = req.path.trim_end_matches('/').to_lowercase();
    if req.method == "GET" {
        if !ok_key {
            respond(&mut out, "401 Unauthorized", "text/html", &page("Kouros на связи", "Телефон видит компьютер, но ключ в адресе не подошёл. Отсканируй QR-код в Kouros ещё раз.", false));
            return;
        }
        live().lock().unwrap().last_check = Some(store::now());
        let _ = app.emit("hae-status", "check");
        respond(&mut out, "200 OK", "text/html", &page("Всё работает", "Телефон видит Kouros на компьютере. Можно настраивать автоматизацию в Health Auto Export.", true));
        return;
    }
    if req.method != "POST" || !(path.ends_with("/hae") || path.is_empty() || path.ends_with("/api")) {
        respond(&mut out, "404 Not Found", "application/json", "{\"ok\":false}");
        return;
    }
    if !ok_key {
        respond(&mut out, "401 Unauthorized", "application/json", "{\"ok\":false,\"error\":\"wrong key\"}");
        return;
    }
    let Some(dir) = inbox(&app) else {
        respond(&mut out, "500 Internal Server Error", "application/json", "{\"ok\":false}");
        return;
    };
    let gz = req.headers.get("content-encoding").map(|v| v.to_lowercase().contains("gzip")).unwrap_or(false);
    let name = format!("{}-{:04}", store::now(), SEQ.fetch_add(1, Ordering::SeqCst) % 10000);
    let tmp = dir.join(format!("{name}.part"));
    let res = (|| -> Result<u64, String> {
        let f = std::fs::File::create(&tmp).map_err(err)?;
        let mut w = std::io::BufWriter::new(f);
        let n = copy_body(&mut r, &req.headers, &mut w)?;
        w.flush().map_err(err)?;
        Ok(n)
    })();
    match res {
        Ok(n) if n > 0 => {
            let fin = dir.join(format!("{name}{}.json", if gz { "-gz" } else { "" }));
            if gz {
                // Unpack now so the importer always reads plain JSON.
                let ok = std::fs::File::open(&tmp).ok().and_then(|f| {
                    let mut d = flate2::read::GzDecoder::new(f);
                    let mut o = std::fs::File::create(&fin).ok()?;
                    std::io::copy(&mut d, &mut o).ok()
                });
                let _ = std::fs::remove_file(&tmp);
                if ok.is_none() {
                    respond(&mut out, "400 Bad Request", "application/json", "{\"ok\":false}");
                    return;
                }
            } else if std::fs::rename(&tmp, &fin).is_err() {
                respond(&mut out, "500 Internal Server Error", "application/json", "{\"ok\":false}");
                return;
            }
            live().lock().unwrap().last_push = Some(store::now());
            respond(&mut out, "200 OK", "application/json", "{\"ok\":true}");
            drop(out);
            drain(&app);
            let _ = app.emit("hae-status", "push");
        }
        _ => {
            let _ = std::fs::remove_file(&tmp);
            respond(&mut out, "400 Bad Request", "application/json", "{\"ok\":false}");
        }
    }
}

/// (Re)starts the receiver with the current settings; stops it when switched off.
pub fn restart(app: &AppHandle) {
    let gen = STOP.fetch_add(1, Ordering::SeqCst) + 1;
    let conf = with_conn(app, |c| Ok(cfg(c))).unwrap_or(Value::Null);
    let enabled = conf["enabled"].as_bool().unwrap_or(false);
    let port = conf["port"].as_u64().map(|x| x as u16).unwrap_or(DEFAULT_PORT);
    let token = conf["token"].as_str().unwrap_or("").to_string();
    {
        let mut l = live().lock().unwrap();
        l.running = false;
        l.port = port;
        l.error = None;
    }
    if !enabled || token.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // The previous listener notices the new generation within 200 ms and frees the port.
        let mut listener = None;
        for _ in 0..15 {
            match TcpListener::bind(("0.0.0.0", port)) {
                Ok(l) => {
                    listener = Some(l);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(300)),
            }
        }
        let Some(listener) = listener else {
            live().lock().unwrap().error = Some(format!("Порт {port} занят другой программой — выбери другой номер"));
            let _ = app.emit("hae-status", "error");
            return;
        };
        let _ = listener.set_nonblocking(true);
        live().lock().unwrap().running = true;
        let _ = app.emit("hae-status", "running");
        while STOP.load(Ordering::SeqCst) == gen {
            match listener.accept() {
                Ok((s, _)) => {
                    let _ = s.set_nonblocking(false);
                    let (a, t) = (app.clone(), token.clone());
                    std::thread::spawn(move || handle(a, s, t));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(200)),
                Err(_) => std::thread::sleep(Duration::from_millis(500)),
            }
        }
        if STOP.load(Ordering::SeqCst) == gen {
            live().lock().unwrap().running = false;
        }
    });
}

/// Background: the inbox and the folder every minute (only while the app is unlocked).
pub fn start(app: AppHandle) {
    let started = AtomicBool::new(false);
    std::thread::spawn(move || {
        let mut last_folder = 0i64;
        loop {
            std::thread::sleep(Duration::from_secs(20));
            let unlocked = *app.state::<AppState>().unlocked.lock().unwrap() == Some(true);
            if !unlocked {
                continue;
            }
            if !started.swap(true, Ordering::SeqCst) {
                restart(&app);
            }
            drain(&app);
            if store::now() - last_folder >= 60 {
                last_folder = store::now();
                if let Err(e) = scan_folder(&app) {
                    if e != "locked" {
                        live().lock().unwrap().error = Some(e);
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------- commands' helpers

/// What Kouros needs from Health Auto Export: (metric, name, important).
const COVER: [(&str, &str, bool); 16] = [
    ("sleep_analysis_raw", "Сон по фазам", true),
    ("hrv_sdnn_raw", "HRV", true),
    ("heart_rate_raw", "Пульс", true),
    ("resting_heart_rate_merged_avg", "Пульс покоя", true),
    ("respiratory_rate_merged_avg", "Дыхание во сне", true),
    ("blood_oxygen", "Кислород", false),
    ("wrist_temp", "Температура запястья", false),
    ("steps_total", "Шаги", true),
    ("active_energy_total", "Активные калории", true),
    ("vo2_max", "VO₂max", false),
    ("body_mass", "Вес", false),
    ("food_entry_energy", "Еда", false),
    ("drink_entry_volume", "Вода", false),
    ("food_entry_caffeine", "Кофеин", false),
    ("running_power_raw", "Беговая динамика", false),
    ("walking_speed_raw", "Ходьба", false),
];

/// Which of the needed data has arrived and when last (only once something came at all).
fn coverage(conn: &Connection) -> Vec<Value> {
    let list = COVER.iter().map(|c| format!("'{}'", c.0)).collect::<Vec<_>>().join(",");
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    if let Ok(mut st) = conn.prepare(&format!("SELECT metric, max(date) FROM health_metrics WHERE provider = ?1 AND metric IN ({list}) GROUP BY metric")) {
        if let Ok(rows) = st.query_map(params![HAE], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            last.extend(rows.flatten());
        }
    }
    COVER.iter().map(|(m, name, imp)| json!({"key": m, "name": name, "important": imp, "last": last.get(*m)})).collect()
}

pub fn state(app: &AppHandle, conn: &Connection) -> Value {
    let mut v = cfg(conn);
    let port = v["port"].as_u64().unwrap_or(DEFAULT_PORT as u64);
    let token = v["token"].as_str().unwrap_or("").to_string();
    let ip = lan_ip();
    let base = ip.as_ref().map(|ip| format!("http://{ip}:{port}"));
    let l = live().lock().unwrap();
    let (first, last, rows): (Option<String>, Option<String>, i64) = conn
        .query_row("SELECT min(date), max(date), count(*) FROM health_metrics WHERE provider = ?1", params![HAE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap_or((None, None, 0));
    let waiting = inbox(app).and_then(|d| std::fs::read_dir(d).ok()).map(|r| r.flatten().filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false)).count()).unwrap_or(0);
    if let Some(o) = v.as_object_mut() {
        o.remove("seen");
    }
    let cover: Vec<Value> = if rows > 0 { coverage(conn) } else { Vec::new() };
    json!({
        "enabled": v["enabled"].as_bool().unwrap_or(false),
        "port": port,
        "token": token,
        "folder": v["folder"].as_str().unwrap_or(""),
        "ip": ip,
        "url": base.as_ref().map(|b| format!("{b}/hae?key={token}")),
        "checkUrl": base.as_ref().map(|b| format!("{b}/check?key={token}")),
        "running": l.running,
        "error": l.error,
        "lastPush": l.last_push,
        "lastCheck": l.last_check,
        "progress": l.progress,
        "lastImport": v["lastImport"],
        "waiting": waiting,
        "data": {"first": first, "last": last, "rows": rows},
        "coverage": cover,
        "groups": crate::sources::GROUPS.iter().map(|g| json!({"key": g.0, "name": g.1, "current": crate::sources::current(conn, g.0)})).chain(std::iter::once(json!({"key": crate::sources::OTHER, "name": crate::sources::OTHER_NAME, "current": crate::sources::current(conn, crate::sources::OTHER)}))).collect::<Vec<_>>(),
    })
}

pub fn set(conn: &Connection, patch: &Value) -> Result<(), String> {
    let mut v = cfg(conn);
    if v["token"].as_str().unwrap_or("").is_empty() || patch["newToken"].as_bool() == Some(true) {
        v["token"] = json!(new_token());
    }
    if let Some(e) = patch["enabled"].as_bool() {
        v["enabled"] = json!(e);
    }
    if let Some(pn) = patch["port"].as_u64() {
        if !(1024..=65535).contains(&pn) {
            return Err("Порт — число от 1024 до 65535".into());
        }
        v["port"] = json!(pn);
    }
    if let Some(f) = patch["folder"].as_str() {
        if f != v["folder"].as_str().unwrap_or("") {
            v["seen"] = json!({});
        }
        v["folder"] = json!(f.trim());
    }
    save_cfg(conn, &v)
}
