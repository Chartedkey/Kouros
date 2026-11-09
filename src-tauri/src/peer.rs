//! Windows hosts an opt-in LAN endpoint; Android initiates every exchange.
//! The pairing key stays in OS storage, and every HTTP body is AES-GCM encrypted.
use crate::{db, peer_data as data, peer_files as files, peer_wire as wire, secrets, AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
const PORT: u16 = 8766;
const SECRET: &str = "kouros_peer_code_v1";
static HOST: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static PAIR_UNTIL: AtomicU64 = AtomicU64::new(0);
fn replay() -> &'static Mutex<std::collections::BTreeMap<String, i64>> {
    static R: OnceLock<Mutex<std::collections::BTreeMap<String, i64>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Default::default()))
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> i64 {
    crate::store::now()
}
fn with<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Connection, &std::path::Path) -> Result<T, String>,
) -> Result<T, String> {
    let st = app.state::<AppState>();
    crate::guard(&st)?;
    let root = crate::root(&st)?;
    let mut lock = st.db.lock().map_err(err)?;
    f(lock.as_mut().ok_or("База недоступна")?, &root)
}
fn notify(app: &AppHandle, text: &str) {
    let _ = app.emit("peer-progress", text);
}
pub fn status(app: &AppHandle) -> Result<Value, String> {
    with(app, |c, _| {
        let conflicts: i64 = c
            .query_row(
                "SELECT count(*)FROM peer_conflicts WHERE resolved=0",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        Ok(
            json!({"hostRunning":HOST.load(Ordering::SeqCst)&&data::state(c,"enabled").as_deref()==Some("1"),"address":crate::hae::lan_ip().map(|ip|format!("{ip}:{PORT}")),"paired":data::state(c,"remote_actor").is_some(),"ready":data::state(c,"ready").as_deref()==Some("1"),"remote":data::state(c,"remote_address"),"lastSync":data::state(c,"last_sync"),"error":data::state(c,"error"),"busy":BUSY.load(Ordering::SeqCst),"conflicts":conflicts+data::state(c,"remote_conflicts").and_then(|v|v.parse::<i64>().ok()).unwrap_or(0),"counts":data::counts(c)?}),
        )
    })
}
pub fn enable(app: &AppHandle) -> Result<Value, String> {
    if cfg!(target_os = "android") {
        return Err("Покажите код на Windows, затем введите его на Android".into());
    }
    let code = match secrets::get(SECRET)? {
        Some(s) => s,
        None => {
            let s = data::random_hex(16)?;
            secrets::set(SECRET, &s)?;
            s
        }
    };
    with(app, |c, root| {
        if data::state(c, "enabled").as_deref() != Some("1") {
            db::backup_now(c, root)?;
        }
        data::set_state(c, "enabled", "1")
    })?;
    PAIR_UNTIL.store((now() + 300) as u64, Ordering::SeqCst);
    start_host(app.clone())?;
    Ok(
        json!({"code":code,"address":crate::hae::lan_ip().map(|ip|format!("{ip}:{PORT}")),"expires":now()+300}),
    )
}
pub fn update_address(app: &AppHandle, value: &str) -> Result<(), String> {
    let address = validate_address(value)?;
    with(app, |c, _| {
        if data::state(c, "ready").as_deref() != Some("1") {
            return Err("Сначала перенесите историю".into());
        }
        data::set_state(c, "remote_address", &address)
    })
}
pub fn forget(app: &AppHandle) -> Result<(), String> {
    with(app, |c, _| {
        c.execute("DELETE FROM peer_state WHERE k NOT IN('file_cache')", [])
            .map_err(err)?;
        c.execute("DELETE FROM peer_stage", []).map_err(err)?;
        c.execute_batch("DROP TABLE IF EXISTS peer_outbox;")
            .map_err(err)?;
        Ok(())
    })?;
    secrets::delete(SECRET)?;
    PAIR_UNTIL.store(0, Ordering::SeqCst);
    Ok(())
}
fn manifest(app: &AppHandle) -> Result<Vec<files::FileInfo>, String> {
    with(app, |c, root| {
        let cache = data::state(c, "file_cache")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let (list, cache) = files::manifest(root, &cache)?;
        data::set_state(c, "file_cache", &cache.to_string())?;
        Ok(list)
    })
}
fn schema(c: &Connection) -> i64 {
    db::schema_version(c)
}
fn freeze(c: &mut Connection, root: &std::path::Path) -> Result<Value, String> {
    db::backup_now(c, root)?;
    let id = data::random_hex(16)?;
    let tx = c.transaction().map_err(err)?;
    tx.execute_batch("DROP TABLE IF EXISTS peer_outbox;CREATE TABLE peer_outbox(n INTEGER PRIMARY KEY,body TEXT NOT NULL);").map_err(err)?;
    let upper = data::tick(&tx)?;
    let mut after = 0;
    let mut n = 0;
    loop {
        let p = data::page(&tx, after, Some(upper))?;
        for mut ch in p.changes {
            if ch.deleted {
                continue;
            }
            files::wire_paths(&mut ch, root, false)?;
            n += 1;
            tx.execute(
                "INSERT INTO peer_outbox(n,body)VALUES(?1,?2)",
                params![n, serde_json::to_string(&ch).map_err(err)?],
            )
            .map_err(err)?;
        }
        after = p.cursor;
        if p.done {
            break;
        }
    }
    data::set_state(&tx, "export_id", &id)?;
    data::set_state(&tx, "export_upper", &upper.to_string())?;
    let counts = data::counts(&tx)?;
    tx.commit().map_err(err)?;
    Ok(json!({"id":id,"upper":upper,"total":n,"counts":counts}))
}
fn dispatch(app: &AppHandle, actor: &str, body: &Value) -> Result<Value, String> {
    let action = body["action"].as_str().ok_or("Нет действия")?;
    if action == "hello" {
        return with(app, |c, root| {
            if body["schema"].as_i64() != Some(schema(c)) {
                return Err("Обновите Kouros на Windows и Android до совместимых версий".into());
            }
            let peer = data::state(c, "remote_actor");
            if peer.as_deref().is_some_and(|p| p != actor) {
                return Err(
                    "С этим компьютером уже сопряжён другой телефон. Сначала отключите сопряжение"
                        .into(),
                );
            }
            if peer.is_none() && now() as u64 > PAIR_UNTIL.load(Ordering::SeqCst) {
                return Err("Код сопряжения истёк. Покажите его на Windows ещё раз".into());
            }
            data::set_state(c, "remote_actor", actor)?;
            let snapshot = if body["initial"] == true {
                Some(freeze(c, root)?)
            } else {
                if data::state(c, "ready").as_deref() != Some("1") {
                    return Err("Сначала перенесите историю с Windows".into());
                }
                None
            };
            let prefix: i64 = c
                .query_row("SELECT id_prefix FROM peer_control", [], |r| r.get(0))
                .map_err(err)?;
            Ok(
                json!({"actor":data::actor(c)?,"schema":schema(c),"snapshot":snapshot,"idPrefix":prefix,"conflicts":conflict_count(c)?}),
            )
        });
    }
    with(app, |c, _| {
        if data::state(c, "remote_actor").as_deref() != Some(actor) {
            return Err("Устройства не сопряжены".into());
        }
        Ok(())
    })?;
    match action {
        "initial_page" => with(app, |c, _| {
            if data::state(c, "export_id").as_deref() != body["id"].as_str() {
                return Err("Перенос прерван. Начните перенос заново".into());
            }
            let after = body["after"].as_i64().ok_or("Нет курсора")?;
            let mut stmt = c
                .prepare("SELECT n,body FROM peer_outbox WHERE n>?1 ORDER BY n LIMIT 100")
                .map_err(err)?;
            let rows = stmt
                .query_map([after], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let mut cursor = after;
            let mut bytes = 0;
            let mut changes = Vec::new();
            for (n, body) in rows {
                if bytes + body.len() > 8 * 1024 * 1024 && !changes.is_empty() {
                    break;
                }
                bytes += body.len();
                changes.push(serde_json::from_str::<Value>(&body).map_err(err)?);
                cursor = n;
            }
            let remaining: i64 = c
                .query_row(
                    "SELECT count(*) FROM peer_outbox WHERE n>?1",
                    [cursor],
                    |r| r.get(0),
                )
                .map_err(err)?;
            Ok(json!({"changes":changes,"cursor":cursor,"done":remaining==0}))
        }),
        "initial_finish" => with(app, |c, _| {
            if data::state(c, "export_id").as_deref() != body["id"].as_str() {
                return Err("История переноса недоступна".into());
            }
            data::set_state(c, "ready", "1")?;
            data::set_state(
                c,
                "sent",
                &data::state(c, "export_upper").unwrap_or_default(),
            )?;
            data::set_state(c, "last_sync", &now().to_string())?;
            c.execute_batch("DROP TABLE IF EXISTS peer_outbox;")
                .map_err(err)?;
            Ok(json!({"ok":true}))
        }),
        "pull" => with(app, |c, root| {
            let after = body["after"].as_i64().ok_or("Нет курсора")?;
            data::set_state(c, "sent", &after.to_string())?;
            let mut p = data::page(c, after, body["upper"].as_i64())?;
            for ch in &mut p.changes {
                files::wire_paths(ch, root, false)?;
            }
            serde_json::to_value(p).map_err(err)
        }),
        "push" => with(app, |c, root| {
            let mut changes: Vec<data::Change> =
                serde_json::from_value(body["changes"].clone()).map_err(err)?;
            if changes.len() > 200 {
                return Err("Слишком много записей".into());
            }
            for ch in &mut changes {
                files::wire_paths(ch, root, true)?;
            }
            let cache = data::state(c, "file_cache")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            files::materialize_refs(root, &changes, &cache)?;
            files::verify_changes(root, &changes)?;
            let n = data::apply(c, &changes, false)?;
            if n > 0 {
                refresh(c)?;
                let _ = app.emit("data-changed", "peer");
            }
            data::set_state(c, "last_sync", &now().to_string())?;
            Ok(json!({"applied":n,"conflicts":conflict_count(c)?}))
        }),
        "conflicts" => local_conflicts(app),
        "resolve" => {
            local_resolve(
                app,
                body["id"].as_i64().ok_or("Нет конфликта")?,
                body["choice"].as_str().ok_or("Нет выбора")?,
            )?;
            with(app, |c, _| {
                Ok(json!({"ok":true,"conflicts":conflict_count(c)?}))
            })
        }
        "file_list" => Ok(json!({"files":manifest(app)?})),
        "file_get" => {
            let info: files::FileInfo =
                serde_json::from_value(body["file"].clone()).map_err(err)?;
            let offset = body["offset"].as_u64().ok_or("Нет позиции файла")?;
            let root = with(app, |_, r| Ok(r.to_path_buf()))?;
            let path = files::source_path(&root, &info)?;
            let mut input = std::fs::File::open(path).map_err(err)?;
            input.seek(SeekFrom::Start(offset)).map_err(err)?;
            let mut bytes = vec![0; 1024 * 1024];
            let n = input.read(&mut bytes).map_err(err)?;
            bytes.truncate(n);
            Ok(json!({"bytes":STANDARD.encode(bytes)}))
        }
        "file_put" => {
            let info: files::FileInfo =
                serde_json::from_value(body["file"].clone()).map_err(err)?;
            let offset = body["offset"].as_u64().ok_or("Нет позиции")?;
            let encoded = body["bytes"].as_str().ok_or("Нет вложения")?;
            if encoded.len() > 1_500_000 {
                return Err("Слишком большой блок файла".into());
            }
            let bytes = STANDARD.decode(encoded).map_err(err)?;
            let root = with(app, |_, r| Ok(r.to_path_buf()))?;
            files::write_chunk(&root, &info, offset, &bytes, body["finish"] == true)?;
            Ok(json!({"ok":true}))
        }
        _ => Err("Неизвестное действие".into()),
    }
}
fn refresh(c: &Connection) -> Result<(), String> {
    c.execute("DELETE FROM activity_metrics", []).map_err(err)?;
    c.execute("DELETE FROM muscle_cache", []).map_err(err)?;
    crate::sources::install_view(c)?;
    crate::workouts_svc::install_view(c)?;
    crate::exercise_svc::load_custom(c);
    Ok(())
}
fn serve(app: &AppHandle, mut s: TcpStream) {
    let result = (|| {
        let code = secrets::get(SECRET)?.ok_or("Приём выключен")?;
        with(app, |c, _| {
            if data::state(c, "enabled").as_deref() != Some("1") {
                return Err("Приём выключен".into());
            }
            Ok(())
        })?;
        let envelope = wire::read_http(&mut s)?;
        let bytes = wire::open(&code, wire::request_aad(), &envelope)?;
        let packet: Value = serde_json::from_slice(&bytes).map_err(err)?;
        let at = packet["at"].as_i64().ok_or("Нет времени")?;
        if (at as i128 - now() as i128).abs() > 300 {
            return Err("Проверьте время на обоих устройствах".into());
        }
        let actor = packet["actor"]
            .as_str()
            .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("Нет устройства")?;
        {
            let mut seen = replay().lock().map_err(err)?;
            seen.retain(|_, t| now() - *t < 600);
            if seen.contains_key(&envelope.nonce) {
                return Err("Повторный запрос".into());
            }
            if seen.len() > 10000 {
                return Err("Слишком много запросов".into());
            }
            seen.insert(envelope.nonce.clone(), now());
        }
        let answer = match dispatch(app, actor, &packet["body"]) {
            Ok(v) => json!({"ok":true,"result":v}),
            Err(e) => json!({"ok":false,"error":e}),
        };
        let sealed = wire::seal(
            &code,
            &wire::response_aad(&envelope.nonce),
            &serde_json::to_vec(&answer).map_err(err)?,
        )?;
        serde_json::to_vec(&sealed).map_err(err)
    })();
    match result {
        Ok(body) => {
            let _=write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",body.len());
            let _ = s.write_all(&body);
        }
        Err(_) => {
            let _ = s.write_all(
                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    }
}
fn start_host(app: AppHandle) -> Result<(), String> {
    if HOST.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
        Ok(l) => l,
        Err(e) => {
            HOST.store(false, Ordering::SeqCst);
            return Err(format!("Не удалось открыть порт {PORT}: {e}"));
        }
    };
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let allowed = s.peer_addr().ok().is_some_and(
                |a| matches!(a.ip(),std::net::IpAddr::V4(v)if v.is_private()||v.is_loopback()),
            );
            if allowed {
                serve(&app, s);
            }
        }
        HOST.store(false, Ordering::SeqCst);
    });
    Ok(())
}
fn validate_address(input: &str) -> Result<String, String> {
    let s = input.trim();
    let (ip, port) = s.split_once(':').unwrap_or((s, "8766"));
    let ip = wire::private_ipv4(ip)?;
    let port: u16 = port.parse().map_err(|_| "Неверный порт")?;
    if port != PORT {
        return Err(format!("Используйте порт {PORT}"));
    }
    Ok(format!("{ip}:{port}"))
}
async fn request(
    client: &reqwest::Client,
    address: &str,
    code: &str,
    actor: &str,
    body: Value,
) -> Result<Value, String> {
    let packet = json!({"at":now(),"actor":actor,"body":body});
    let encrypted = wire::seal(
        code,
        wire::request_aad(),
        &serde_json::to_vec(&packet).map_err(err)?,
    )?;
    let response=client.post(format!("http://{address}/kouros-sync/v1")).json(&encrypted).send().await.map_err(|_|"Не удалось связаться с Windows. Откройте оба приложения в одной Wi-Fi-сети и проверьте разрешение частной сети в брандмауэре")?;
    if !response.status().is_success() {
        return Err("Код не подошёл, приём выключен или Kouros на Windows заблокирован".into());
    }
    let mut r = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = r.chunk().await.map_err(err)? {
        if bytes.len() + chunk.len() > wire::MAX_WIRE {
            return Err("Слишком большой ответ".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let envelope = serde_json::from_slice(&bytes).map_err(err)?;
    let bytes = wire::open(code, &wire::response_aad(&encrypted.nonce), &envelope)?;
    let answer: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if answer["ok"] != true {
        return Err(answer["error"].as_str().unwrap_or("Ошибка обмена").into());
    }
    Ok(answer["result"].clone())
}
struct Session {
    client: reqwest::Client,
    address: String,
    code: String,
    actor: String,
}
impl Session {
    async fn call(&self, body: Value) -> Result<Value, String> {
        request(&self.client, &self.address, &self.code, &self.actor, body).await
    }
}
async fn session(app: &AppHandle) -> Result<Session, String> {
    let (address, actor) = with(app, |c, _| {
        Ok((
            data::state(c, "remote_address").ok_or("Сначала сопрягите устройства")?,
            data::actor(c)?,
        ))
    })?;
    let code = secrets::get(SECRET)?.ok_or("Код сопряжения не найден")?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(err)?;
    Ok(Session {
        client,
        address: validate_address(&address)?,
        code,
        actor,
    })
}
async fn transfer_files(app: &AppHandle, s: &Session, initial: bool) -> Result<(), String> {
    let local = manifest(app)?;
    let remote: Vec<files::FileInfo> =
        serde_json::from_value(s.call(json!({"action":"file_list"})).await?["files"].clone())
            .map_err(err)?;
    let root = with(app, |_, r| Ok(r.to_path_buf()))?;
    if !initial {
        for file in &local {
            if remote
                .iter()
                .any(|r| r.path == file.path && r.sha256 == file.sha256)
            {
                continue;
            }
            notify(app, "Передаю вложения на Windows…");
            let path = files::source_path(&root, file)?;
            let mut f = std::fs::File::open(path).map_err(err)?;
            let mut offset = 0;
            loop {
                let mut b = vec![0; 1024 * 1024];
                let n = f.read(&mut b).map_err(err)?;
                b.truncate(n);
                s.call(json!({"action":"file_put","file":file,"offset":offset,"bytes":STANDARD.encode(&b),"finish":offset+n as u64==file.size})).await?;
                offset += n as u64;
                if offset == file.size {
                    break;
                }
                if n == 0 {
                    return Err("Вложение изменилось во время передачи".into());
                }
            }
        }
    }
    for file in &remote {
        let dest = files::safe_path(&root, &file.path)?;
        if dest.is_file() && files::hash(&dest)? == file.sha256 {
            continue;
        }
        notify(app, "Получаю вложения с Windows…");
        let mut offset = 0;
        loop {
            let answer = s
                .call(json!({"action":"file_get","file":file,"offset":offset}))
                .await?;
            let bytes = STANDARD
                .decode(answer["bytes"].as_str().ok_or("Нет вложения")?)
                .map_err(err)?;
            let finish = offset + bytes.len() as u64 == file.size;
            files::write_chunk(&root, file, offset, &bytes, finish)?;
            offset += bytes.len() as u64;
            if finish {
                break;
            }
            if bytes.is_empty() {
                return Err("Вложение передано не полностью".into());
            }
        }
    }
    Ok(())
}
fn mark_error(app: &AppHandle, e: &str) {
    let _ = with(app, |c, _| data::set_state(c, "error", e));
}
struct Busy;
impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}
fn busy() -> Result<Busy, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("Синхронизация уже выполняется".into());
    }
    Ok(Busy)
}
pub async fn pair(
    app: AppHandle,
    remote: String,
    code: String,
    replace: bool,
) -> Result<Value, String> {
    let _busy = busy()?;
    if !cfg!(target_os = "android") {
        return Err("Первый перенос запускается на Android".into());
    }
    if !replace {
        return Err("Подтвердите перенос истории Windows на Android".into());
    }
    let remote = validate_address(&remote)?;
    let code = wire::normalized_code(&code)?;
    secrets::set(SECRET, &code)?;
    with(&app, |c, _| {
        data::set_state(c, "ready", "0")?;
        data::set_state(c, "remote_address", &remote)
    })?;
    let result = initial(&app).await;
    if let Err(ref e) = result {
        mark_error(&app, e)
    }
    result
}
async fn initial(app: &AppHandle) -> Result<Value, String> {
    let s = session(app).await?;
    let schema = with(app, |c, _| Ok(schema(c)))?;
    notify(app, "Проверяю соединение с Windows…");
    let hello = s
        .call(json!({"action":"hello","schema":schema,"initial":true}))
        .await?;
    let snapshot = &hello["snapshot"];
    let id = snapshot["id"]
        .as_str()
        .ok_or("Нет снимка истории")?
        .to_string();
    let upper = snapshot["upper"].as_i64().ok_or("Нет курсора истории")?;
    let mut backup = with(app, |c, root| {
        let backup = db::backup_now(c, root)?;
        c.execute("DELETE FROM peer_stage", []).map_err(err)?;
        let prefix: i64 = c
            .query_row("SELECT id_prefix FROM peer_control", [], |r| r.get(0))
            .map_err(err)?;
        let mut prefix = prefix;
        while hello["idPrefix"].as_i64() == Some(prefix) {
            c.execute("UPDATE peer_control SET id_prefix=0,id_next=0", [])
                .map_err(err)?;
            data::init(c)?;
            prefix = c
                .query_row("SELECT id_prefix FROM peer_control", [], |r| r.get(0))
                .map_err(err)?;
        }
        Ok(backup)
    })?;
    let mut after = 0;
    loop {
        notify(
            app,
            &format!("Получаю историю: {after} / {} записей…", snapshot["total"]),
        );
        let p = s
            .call(json!({"action":"initial_page","id":id,"after":after}))
            .await?;
        let mut changes: Vec<data::Change> =
            serde_json::from_value(p["changes"].clone()).map_err(err)?;
        with(app, |c, root| {
            let tx = c.transaction().map_err(err)?;
            for ch in &mut changes {
                data::columns(&tx, &ch.t)?;
                files::wire_paths(ch, root, true)?;
                tx.execute(
                    "INSERT INTO peer_stage(body)VALUES(?1)",
                    [serde_json::to_string(ch).map_err(err)?],
                )
                .map_err(err)?;
            }
            tx.commit().map_err(err)
        })?;
        after = p["cursor"].as_i64().ok_or("Нет курсора")?;
        if p["done"] == true {
            break;
        }
    }
    let staged = with(app, |c, _| {
        c.query_row("SELECT count(*)FROM peer_stage", [], |r| r.get::<_, i64>(0))
            .map_err(err)
    })?;
    if snapshot["total"].as_i64() != Some(after) || staged != after {
        return Err("История передана не полностью. Повторите перенос".into());
    }
    transfer_files(app, &s, true).await?;
    with(app, |c, root| files::verify_stage(root, c))?;
    notify(app, "Проверяю и сохраняю историю…");
    let applied = with(app, |c, root| {
        backup = db::backup_now(c, root)?;
        let updates = [
            (
                "remote_actor",
                hello["actor"].as_str().ok_or("Нет устройства")?.to_string(),
            ),
            ("received", upper.to_string()),
            ("ready", "1".into()),
            ("pending_finish", id.clone()),
            ("last_sync", now().to_string()),
            ("error", String::new()),
        ];
        let n = data::replace_with_state(c, &updates)?;
        refresh(c)?;
        c.execute("DELETE FROM peer_stage", []).map_err(err)?;
        Ok(n)
    })?;
    s.call(json!({"action":"initial_finish","id":id})).await?;
    with(app, |c, _| data::set_state(c, "pending_finish", ""))?;
    let _ = app.emit("data-changed", "peer");
    notify(app, "История перенесена. Устройства сопряжены.");
    Ok(json!({"applied":applied,"backup":backup}))
}
pub async fn sync(app: AppHandle) -> Result<Value, String> {
    let _busy = busy()?;
    let result = exchange(&app).await;
    if let Err(ref e) = result {
        mark_error(&app, e)
    }
    result
}
async fn exchange(app: &AppHandle) -> Result<Value, String> {
    if !with(app, |c, _| {
        Ok(data::state(c, "ready").as_deref() == Some("1"))
    })? {
        return Err("Сначала перенесите историю Windows".into());
    }
    let s = session(app).await?;
    if let Some(id) =
        with(app, |c, _| Ok(data::state(c, "pending_finish")))?.filter(|s| !s.is_empty())
    {
        s.call(json!({"action":"initial_finish","id":id})).await?;
        with(app, |c, _| data::set_state(c, "pending_finish", ""))?;
    }
    notify(app, "Обмениваюсь изменениями…");
    let schema = with(app, |c, _| Ok(schema(c)))?;
    let hello = s
        .call(json!({"action":"hello","schema":schema,"initial":false}))
        .await?;
    with(app, |c, _| {
        if data::state(c, "remote_actor").as_deref() != hello["actor"].as_str() {
            return Err("Это другой компьютер. Сопрягите устройства заново".into());
        }
        data::set_state(
            c,
            "remote_conflicts",
            &hello["conflicts"].as_i64().unwrap_or(0).to_string(),
        )?;
        Ok(())
    })?;
    transfer_files(app, &s, false).await?;
    let mut sent = 0;
    let mut received = 0;
    let mut cursor = with(app, |c, _| {
        Ok(data::state(c, "sent")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    })?;
    let mut upper = None;
    loop {
        let p = with(app, |c, root| {
            let mut p = data::page(c, cursor, upper)?;
            for ch in &mut p.changes {
                files::wire_paths(ch, root, false)?;
            }
            Ok(p)
        })?;
        if !p.changes.is_empty() {
            let result = s.call(json!({"action":"push","changes":p.changes})).await?;
            sent += result["applied"].as_i64().unwrap_or(0);
            with(app, |c, _| {
                data::set_state(
                    c,
                    "remote_conflicts",
                    &result["conflicts"].as_i64().unwrap_or(0).to_string(),
                )
            })?;
        }
        cursor = p.cursor;
        upper = Some(p.upper);
        with(app, |c, _| data::set_state(c, "sent", &cursor.to_string()))?;
        if p.done {
            break;
        }
    }
    let mut cursor = with(app, |c, _| {
        Ok(data::state(c, "received")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    })?;
    let mut upper = None;
    loop {
        let p = s
            .call(json!({"action":"pull","after":cursor,"upper":upper}))
            .await?;
        let mut p: data::Page = serde_json::from_value(p).map_err(err)?;
        with(app, |c, root| {
            for ch in &mut p.changes {
                files::wire_paths(ch, root, true)?;
            }
            let cache = data::state(c, "file_cache")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            files::materialize_refs(root, &p.changes, &cache)?;
            files::verify_changes(root, &p.changes)?;
            received += data::apply(c, &p.changes, false)? as i64;
            data::set_state(c, "received", &p.cursor.to_string())
        })?;
        cursor = p.cursor;
        upper = Some(p.upper);
        if p.done {
            break;
        }
    }
    with(app, |c, _| {
        if received > 0 {
            refresh(c)?;
        }
        data::set_state(c, "last_sync", &now().to_string())?;
        data::set_state(c, "error", "")
    })?;
    let _ = app.emit("data-changed", "peer");
    notify(app, "Синхронизация завершена");
    Ok(json!({"sent":sent,"received":received}))
}
fn conflict_count(c: &Connection) -> Result<i64, String> {
    c.query_row(
        "SELECT count(*)FROM peer_conflicts WHERE resolved=0",
        [],
        |r| r.get(0),
    )
    .map_err(err)
}
fn conflict_preview(text: &str) -> String {
    fn short(v: Value, depth: u8) -> Value {
        match v {
            Value::String(s) => Value::String(if s.chars().count() > 512 {
                s.chars().take(512).collect::<String>() + "…"
            } else {
                s
            }),
            Value::Array(a) => Value::Array(
                a.into_iter()
                    .take(12)
                    .map(|v| short(v, depth + 1))
                    .collect(),
            ),
            Value::Object(o) if depth < 3 => Value::Object(
                o.into_iter()
                    .take(80)
                    .map(|(k, v)| (k, short(v, depth + 1)))
                    .collect(),
            ),
            Value::Object(_) => Value::String("Подробная запись сохранена на Windows".into()),
            v => v,
        }
    }
    serde_json::from_str::<Value>(text)
        .map(|v| short(v, 0).to_string())
        .unwrap_or_else(|_| "null".into())
}
fn local_conflicts(app: &AppHandle) -> Result<Value, String> {
    with(app, |c, _| {
        let mut s=c.prepare("SELECT id,at,t,k,local,remote FROM peer_conflicts WHERE resolved=0 ORDER BY id DESC LIMIT 50").map_err(err)?;
        let list=s.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"at":r.get::<_,i64>(1)?,"table":r.get::<_,String>(2)?,"key":r.get::<_,String>(3)?,"local":conflict_preview(&r.get::<_,String>(4)?),"remote":conflict_preview(&r.get::<_,String>(5)?)}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        Ok(json!(list))
    })
}
fn local_resolve(app: &AppHandle, id: i64, choice: &str) -> Result<(), String> {
    with(app, |c, _| {
        let (t, key, local, remote): (String, String, String, String) = c
            .query_row(
                "SELECT t,k,local,remote FROM peer_conflicts WHERE id=?1 AND resolved=0",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(err)?;
        if choice != "current" {
            let key = serde_json::from_str(&key).map_err(err)?;
            let row = match choice {
                "local" => serde_json::from_str::<Option<Value>>(&local).map_err(err)?,
                "remote" => {
                    serde_json::from_str::<data::Change>(&remote)
                        .map_err(err)?
                        .row
                }
                _ => return Err("Неверный вариант".into()),
            };
            let ch = data::Change {
                t,
                k: key,
                rev: data::tick(c)? + 1,
                actor: data::actor(c)?,
                deleted: row.is_none(),
                row,
            };
            data::apply(c, &[ch], true)?;
            refresh(c)?;
        }
        c.execute("UPDATE peer_conflicts SET resolved=1 WHERE id=?1", [id])
            .map_err(err)?;
        let _ = app.emit("data-changed", "peer");
        Ok(())
    })
}
pub fn start(app: AppHandle) {
    if !cfg!(target_os = "android") {
        let a = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let enabled = with(&a, |c, _| {
                Ok(data::state(c, "enabled").as_deref() == Some("1"))
            })
            .unwrap_or(false);
            if enabled {
                let _ = start_host(a.clone());
            }
        });
    } else {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let ready = with(&app, |c, _| {
                    Ok(data::state(c, "ready").as_deref() == Some("1"))
                })
                .unwrap_or(false);
                if ready {
                    let _ = sync(app.clone()).await;
                }
            }
        });
    }
}

pub async fn conflicts(app: AppHandle) -> Result<Value, String> {
    if cfg!(target_os = "android") {
        session(&app)
            .await?
            .call(json!({"action":"conflicts"}))
            .await
    } else {
        local_conflicts(&app)
    }
}
pub async fn resolve(app: AppHandle, id: i64, choice: String) -> Result<(), String> {
    if cfg!(target_os = "android") {
        let result = session(&app)
            .await?
            .call(json!({"action":"resolve","id":id,"choice":choice}))
            .await?;
        with(&app, |c, _| {
            data::set_state(
                c,
                "remote_conflicts",
                &result["conflicts"].as_i64().unwrap_or(0).to_string(),
            )
        })
    } else {
        local_resolve(&app, id, &choice)
    }
}
