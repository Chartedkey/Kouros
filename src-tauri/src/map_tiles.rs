//! OpenStreetMap tiles for workout routes, served to the page through the
//! `kmap` protocol. Only the tile address (zoom / x / y) goes out — never the
//! track itself. Tiles are cached in Kouros\cache\tiles, so a route opened again
//! needs no network, and OpenStreetMap's servers are spared repeat requests.
//! Off unless the athlete turns the map on in Settings.

use crate::{db, AppState};
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Manager};

const UA: &str = concat!("Kouros/", env!("CARGO_PKG_VERSION"), " (personal training app; https://github.com/Chartedkey/Kouros)");

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default()
    })
}

fn reply(status: StatusCode, body: Vec<u8>, kind: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", kind)
        .header("Cache-Control", "max-age=604800")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

pub fn cache_dir(root: &std::path::Path) -> PathBuf {
    root.join("cache").join("tiles")
}

/// Map is allowed only after unlock and only when switched on.
fn allowed(app: &AppHandle) -> Option<PathBuf> {
    let st = app.state::<AppState>();
    if *st.unlocked.lock().unwrap() != Some(true) {
        return None;
    }
    let on = st.db.lock().unwrap().as_ref().and_then(|c| db::get_setting(c, "map_online")).map(|v| v == "1").unwrap_or(false);
    if !on {
        return None;
    }
    let dir = st.data_dir.lock().unwrap().clone();
    dir
}

fn parse(path: &str) -> Option<(u32, u32, u32)> {
    let p = path.trim_start_matches('/').trim_end_matches(".png");
    let mut it = p.split('/');
    let z: u32 = it.next()?.parse().ok()?;
    let x: u32 = it.next()?.parse().ok()?;
    let y: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() || z > 19 || x >= (1 << z) || y >= (1 << z) {
        return None;
    }
    Some((z, x, y))
}

pub async fn serve(app: AppHandle, path: String) -> Response<Vec<u8>> {
    let Some((z, x, y)) = parse(&path) else { return reply(StatusCode::BAD_REQUEST, Vec::new(), "text/plain") };
    let Some(root) = allowed(&app) else { return reply(StatusCode::FORBIDDEN, Vec::new(), "text/plain") };
    let file = cache_dir(&root).join(z.to_string()).join(x.to_string()).join(format!("{y}.png"));
    if let Ok(bytes) = std::fs::read(&file) {
        return reply(StatusCode::OK, bytes, "image/png");
    }
    let url = format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png");
    match client().get(&url).send().await {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => {
                let bytes = b.to_vec();
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                    let _ = std::fs::write(&file, &bytes);
                }
                reply(StatusCode::OK, bytes, "image/png")
            }
            Err(_) => reply(StatusCode::BAD_GATEWAY, Vec::new(), "text/plain"),
        },
        Ok(r) => reply(StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY), Vec::new(), "text/plain"),
        Err(_) => reply(StatusCode::BAD_GATEWAY, Vec::new(), "text/plain"),
    }
}

/// (files, bytes) in the tile cache.
pub fn cache_size(root: &std::path::Path) -> (u64, u64) {
    fn walk(p: &std::path::Path, acc: &mut (u64, u64)) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, acc);
            } else if let Ok(m) = e.metadata() {
                acc.0 += 1;
                acc.1 += m.len();
            }
        }
    }
    let mut acc = (0, 0);
    walk(&cache_dir(root), &mut acc);
    acc
}
