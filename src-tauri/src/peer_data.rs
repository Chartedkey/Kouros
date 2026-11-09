//! Local replication of business rows. No credentials, SQL text or filesystem access on the wire.
use rusqlite::{params, params_from_iter, types::Value as Sql, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
pub const TABLES: &[&str] = &[
    "settings",
    "exercises",
    "activities",
    "activity_streams",
    "strength_sets",
    "health_metrics",
    "habit_log",
    "surveys",
    "readiness_log",
    "evening_log",
    "meds",
    "med_log",
    "coach_reports",
    "plan_days",
    "plan_changes",
    "checkins",
    "pain_log",
    "lab_reports",
    "lab_results",
];
const LOCAL: &[&str] = &[
    "lock_enabled",
    "hae",
    "strava_athlete",
    "garmin_raw",
    "provider_backfill",
    "raw_backfill",
    "workout_backfill",
    "workout_raw_backfill",
    "source_backfill",
    "device_seen",
    "recount_sides",
    "peer",
];
const INTS: &[&str] = &[
    "exercises",
    "surveys",
    "meds",
    "med_log",
    "coach_reports",
    "plan_changes",
    "pain_log",
    "lab_reports",
    "lab_results",
];
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn random_hex(n: usize) -> Result<String, String> {
    let mut b = vec![0; n];
    getrandom::getrandom(&mut b).map_err(err)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
#[derive(Clone)]
pub struct Col {
    pub name: String,
    pub pk: i64,
}
pub fn columns(c: &Connection, t: &str) -> Result<Vec<Col>, String> {
    if !TABLES.contains(&t) {
        return Err("Неизвестная таблица синхронизации".into());
    }
    let mut s = c
        .prepare(&format!("PRAGMA table_info(\"{t}\")"))
        .map_err(err)?;
    let v = s
        .query_map([], |r| {
            Ok(Col {
                name: r.get(1)?,
                pk: r.get(5)?,
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(v)
}
fn pks(cols: &[Col]) -> Vec<&Col> {
    let mut v: Vec<_> = cols.iter().filter(|x| x.pk > 0).collect();
    v.sort_by_key(|x| x.pk);
    v
}
fn key_expr(cols: &[Col], prefix: &str) -> String {
    format!(
        "json_array({})",
        pks(cols)
            .iter()
            .map(|x| format!("{prefix}\"{}\"", x.name))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn local_filter(prefix: &str) -> String {
    format!(
        "{prefix}key NOT IN ({}) AND {prefix}key NOT LIKE 'peer_%'",
        LOCAL
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub fn shared_setting(k: &str) -> bool {
    !LOCAL.contains(&k) && !k.starts_with("peer_")
}

pub fn init(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS peer_control(id INTEGER PRIMARY KEY CHECK(id=1),actor TEXT NOT NULL,tick INTEGER NOT NULL DEFAULT 0,muted INTEGER NOT NULL DEFAULT 0,id_prefix INTEGER NOT NULL DEFAULT 0,id_next INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS peer_rows(t TEXT NOT NULL,k TEXT NOT NULL,rev INTEGER NOT NULL,actor TEXT NOT NULL,deleted INTEGER NOT NULL,seq INTEGER NOT NULL,PRIMARY KEY(t,k)); CREATE INDEX IF NOT EXISTS peer_rows_seq ON peer_rows(seq); CREATE TABLE IF NOT EXISTS peer_state(k TEXT PRIMARY KEY,v TEXT NOT NULL); CREATE TABLE IF NOT EXISTS peer_conflicts(id INTEGER PRIMARY KEY,at INTEGER NOT NULL,t TEXT NOT NULL,k TEXT NOT NULL,local TEXT,remote TEXT,resolved INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS peer_stage(n INTEGER PRIMARY KEY,body TEXT NOT NULL);").map_err(err)?;
    c.execute(
        "INSERT OR IGNORE INTO peer_control(id,actor) VALUES(1,?1)",
        [random_hex(16)?],
    )
    .map_err(err)?;
    for &t in TABLES {
        let cols = columns(c, t)?;
        if cols.is_empty() {
            return Err(format!("Нет таблицы {t}"));
        }
        let key = key_expr(&cols, "");
        let filter = if t == "settings" {
            format!("WHERE {}", local_filter(""))
        } else {
            String::new()
        };
        c.execute_batch(&format!("INSERT OR IGNORE INTO peer_rows(t,k,rev,actor,deleted,seq) SELECT '{t}',{key},1,(SELECT actor FROM peer_control),0,(SELECT tick FROM peer_control)+row_number() OVER() FROM \"{t}\" {filter}; UPDATE peer_control SET tick=max(tick,(SELECT coalesce(max(seq),0) FROM peer_rows));")).map_err(err)?;
        for (op, prefix, deleted) in [
            ("INSERT", "NEW.", 0),
            ("UPDATE", "NEW.", 0),
            ("DELETE", "OLD.", 1),
        ] {
            let key = key_expr(&cols, prefix);
            let filter = if t == "settings" {
                format!("AND {}", local_filter(prefix))
            } else {
                String::new()
            };
            let old_key = key_expr(&cols, "OLD.");
            let moved = if op == "UPDATE" {
                format!("UPDATE peer_control SET tick=tick+1 WHERE {old_key}<>{key}; INSERT INTO peer_rows(t,k,rev,actor,deleted,seq)SELECT '{t}',{old_key},tick,actor,1,tick FROM peer_control WHERE id=1 AND {old_key}<>{key} ON CONFLICT(t,k)DO UPDATE SET rev=excluded.rev,actor=excluded.actor,deleted=1,seq=excluded.seq;")
            } else {
                String::new()
            };
            let sql=format!("CREATE TRIGGER IF NOT EXISTS peer_{t}_{op} AFTER {op} ON \"{t}\" WHEN (SELECT muted FROM peer_control)=0 {filter} BEGIN {moved} UPDATE peer_control SET tick=tick+1; INSERT INTO peer_rows(t,k,rev,actor,deleted,seq) SELECT '{t}',{key},tick,actor,{deleted},tick FROM peer_control WHERE id=1 ON CONFLICT(t,k) DO UPDATE SET rev=excluded.rev,actor=excluded.actor,deleted=excluded.deleted,seq=excluded.seq; END;");
            c.execute_batch(&sql).map_err(err)?;
        }
    }
    install_ids(c)?;
    Ok(())
}
fn install_ids(c: &Connection) -> Result<(), String> {
    let (mut prefix, mut start): (i64, i64) = c
        .query_row("SELECT id_prefix,id_next FROM peer_control", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(err)?;
    if prefix == 0 || start + 1_000_000 >= (1i64 << 32) {
        loop {
            let mut b = [0; 4];
            getrandom::getrandom(&mut b).map_err(err)?;
            prefix = (u32::from_le_bytes(b) as i64 & 0xfffff).max(1);
            let mut used = false;
            for t in INTS {
                let count: i64 = c
                    .query_row(
                        &format!("SELECT count(*) FROM {t} WHERE (id >> 32)=?1"),
                        [prefix],
                        |r| r.get(0),
                    )
                    .map_err(err)?;
                used |= count > 0;
            }
            if !used {
                break;
            }
        }
        start = 0;
    }
    c.execute(
        "UPDATE peer_control SET id_prefix=?1,id_next=?2",
        params![prefix, start + 1_000_000],
    )
    .map_err(err)?;
    let counter = Arc::new(AtomicI64::new(start));
    let end = start + 1_000_000;
    c.create_scalar_function(
        "kouros_id",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        move |_| {
            let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
            if next > end {
                return Err(rusqlite::Error::UserFunctionError(
                    "Перезапустите Kouros для продолжения импорта".into(),
                ));
            }
            Ok((prefix << 32) | next)
        },
    )
    .map_err(err)?;
    c.create_scalar_function(
        "kouros_exercise_id",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let name: String = ctx.get(0)?;
            let h = ring::digest::digest(&ring::digest::SHA256, name.as_bytes());
            let mut b = [0u8; 8];
            b[..6].copy_from_slice(&h.as_ref()[..6]);
            Ok(-((i64::from_le_bytes(b) & 0x0000_ffff_ffff_ffff) + 1))
        },
    )
    .map_err(err)?;
    Ok(())
}
pub fn state(c: &Connection, k: &str) -> Option<String> {
    c.query_row("SELECT v FROM peer_state WHERE k=?1", [k], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}
pub fn set_state(c: &Connection, k: &str, v: &str) -> Result<(), String> {
    c.execute(
        "INSERT INTO peer_state(k,v)VALUES(?1,?2) ON CONFLICT(k)DO UPDATE SET v=excluded.v",
        params![k, v],
    )
    .map_err(err)?;
    Ok(())
}
pub fn actor(c: &Connection) -> Result<String, String> {
    c.query_row("SELECT actor FROM peer_control", [], |r| r.get(0))
        .map_err(err)
}
pub fn tick(c: &Connection) -> Result<i64, String> {
    c.query_row("SELECT tick FROM peer_control", [], |r| r.get(0))
        .map_err(err)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub t: String,
    pub k: Vec<Value>,
    pub rev: i64,
    pub actor: String,
    pub deleted: bool,
    pub row: Option<Value>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Page {
    pub changes: Vec<Change>,
    pub cursor: i64,
    pub upper: i64,
    pub done: bool,
}
fn sqlval(v: &Value) -> Result<Sql, String> {
    Ok(match v {
        Value::Null => Sql::Null,
        Value::String(s) => Sql::Text(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Sql::Integer(i)
            } else {
                Sql::Real(n.as_f64().ok_or("Неверное число")?)
            }
        }
        _ => return Err("Неверное поле записи".into()),
    })
}
fn predicate(cols: &[Col], key: &[Value]) -> Result<(String, Vec<Sql>), String> {
    let pk = pks(cols);
    if pk.len() != key.len() {
        return Err("Неверный ключ записи".into());
    }
    Ok((
        pk.iter()
            .map(|x| format!("\"{}\" IS ?", x.name))
            .collect::<Vec<_>>()
            .join(" AND "),
        key.iter().map(sqlval).collect::<Result<_, _>>()?,
    ))
}
pub fn read_row(c: &Connection, t: &str, key: &[Value]) -> Result<Option<Value>, String> {
    let cols = columns(c, t)?;
    let (pred, vals) = predicate(&cols, key)?;
    let expr = format!(
        "json_object({})",
        cols.iter()
            .map(|x| format!("'{}',\"{}\"", x.name, x.name))
            .collect::<Vec<_>>()
            .join(",")
    );
    let text: Option<String> = c
        .query_row(
            &format!("SELECT {expr} FROM \"{t}\" WHERE {pred}"),
            params_from_iter(vals),
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    text.map(|s| serde_json::from_str(&s).map_err(err))
        .transpose()
}
pub fn page(c: &Connection, after: i64, upper: Option<i64>) -> Result<Page, String> {
    let upper = upper.unwrap_or(tick(c)?);
    if after < 0 || upper < after {
        return Err("Неверный курсор".into());
    }
    let mut s=c.prepare("SELECT t,k,rev,actor,deleted,seq FROM peer_rows WHERE seq>?1 AND seq<=?2 ORDER BY seq,t,k LIMIT 200").map_err(err)?;
    let rows = s
        .query_map(params![after, upper], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut changes = Vec::new();
    let mut cursor = after;
    let mut bytes = 0;
    for (t, k, rev, actor, deleted, seq) in rows {
        let key: Vec<Value> = serde_json::from_str(&k).map_err(err)?;
        let row = if deleted {
            None
        } else {
            read_row(c, &t, &key)?
        };
        let size = row.as_ref().map(|v| v.to_string().len()).unwrap_or(0);
        if bytes + size > 8 * 1024 * 1024 && !changes.is_empty() {
            break;
        }
        bytes += size;
        changes.push(Change {
            t,
            k: key,
            rev,
            actor,
            deleted: deleted || row.is_none(),
            row,
        });
        cursor = seq;
    }
    let remaining: i64 = c
        .query_row(
            "SELECT count(*)FROM peer_rows WHERE seq>?1 AND seq<=?2",
            params![cursor, upper],
            |r| r.get(0),
        )
        .map_err(err)?;
    let done = remaining == 0;
    if done {
        cursor = upper
    }
    Ok(Page {
        changes,
        cursor,
        upper,
        done,
    })
}
pub fn apply(c: &mut Connection, changes: &[Change], initial: bool) -> Result<usize, String> {
    let own = actor(c)?;
    let sent = state(c, "sent")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let tx = c.transaction().map_err(err)?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON;UPDATE peer_control SET muted=1;")
        .map_err(err)?;
    let mut list: Vec<_> = changes.iter().collect();
    list.sort_by_key(|v| {
        (
            v.deleted,
            if v.deleted {
                usize::MAX - TABLES.iter().position(|&t| t == v.t).unwrap_or(0)
            } else {
                TABLES.iter().position(|&t| t == v.t).unwrap_or(0)
            },
        )
    });
    let mut applied = 0;
    for change in list {
        let cols = columns(&tx, &change.t)?;
        if change.actor.len() != 32
            || !change.actor.bytes().all(|x| x.is_ascii_hexdigit())
            || change.rev <= 0
            || change.rev > 9_000_000_000_000_000
        {
            return Err("Неверная версия записи".into());
        }
        if change.t == "settings"
            && !change
                .k
                .first()
                .and_then(Value::as_str)
                .is_some_and(shared_setting)
        {
            return Err("Локальная настройка не переносится".into());
        }
        let key = serde_json::to_string(&change.k).map_err(err)?;
        let old: Option<(i64, String, i64)> = tx
            .query_row(
                "SELECT rev,actor,seq FROM peer_rows WHERE t=?1 AND k=?2",
                params![change.t, key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(err)?;
        if old
            .as_ref()
            .is_some_and(|(r, a, _)| *r == change.rev && *a == change.actor)
        {
            continue;
        }
        let wins = initial
            || old
                .as_ref()
                .map(|(r, a, _)| (change.rev, &change.actor) > (*r, a))
                .unwrap_or(true);
        if !initial
            && old
                .as_ref()
                .is_some_and(|(_, a, seq)| *a == own && *seq > sent && *a != change.actor)
        {
            let local = read_row(&tx, &change.t, &change.k)?;
            if local != change.row {
                tx.execute("INSERT INTO peer_conflicts(at,t,k,local,remote)VALUES(unixepoch(),?1,?2,?3,?4)",params![change.t,key,serde_json::to_string(&local).map_err(err)?,serde_json::to_string(change).map_err(err)?]).map_err(err)?;
            }
        }
        if !wins {
            continue;
        }
        let (pred, vals) = predicate(&cols, &change.k)?;
        if change.deleted {
            tx.execute(
                &format!("DELETE FROM \"{}\" WHERE {pred}", change.t),
                params_from_iter(vals),
            )
            .map_err(err)?;
        } else {
            let row = change
                .row
                .as_ref()
                .and_then(Value::as_object)
                .ok_or("Нет содержимого записи")?;
            if row.len() != cols.len() || cols.iter().any(|x| !row.contains_key(&x.name)) {
                return Err("Разные версии структуры базы. Обновите оба приложения".into());
            }
            for (col, value) in pks(&cols).iter().zip(&change.k) {
                if row.get(&col.name) != Some(value) {
                    return Err("Ключ записи не совпадает".into());
                }
            }
            let fields = cols
                .iter()
                .map(|x| format!("\"{}\"", x.name))
                .collect::<Vec<_>>();
            let updates = cols
                .iter()
                .filter(|x| x.pk == 0)
                .map(|x| format!("\"{}\"=excluded.\"{}\"", x.name, x.name))
                .collect::<Vec<_>>()
                .join(",");
            let conflict = if updates.is_empty() {
                "DO NOTHING".into()
            } else {
                format!("DO UPDATE SET {updates}")
            };
            let sql = format!(
                "INSERT INTO \"{}\"({})VALUES({}) ON CONFLICT({}) {conflict}",
                change.t,
                fields.join(","),
                vec!["?"; fields.len()].join(","),
                pks(&cols)
                    .iter()
                    .map(|x| format!("\"{}\"", x.name))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            tx.execute(
                &sql,
                params_from_iter(
                    cols.iter()
                        .map(|x| sqlval(&row[&x.name]))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            )
            .map_err(err)?;
        }
        tx.execute("UPDATE peer_control SET tick=max(tick,?1)+1", [change.rev])
            .map_err(err)?;
        tx.execute("INSERT INTO peer_rows(t,k,rev,actor,deleted,seq)SELECT ?1,?2,?3,?4,?5,tick FROM peer_control WHERE id=1 ON CONFLICT(t,k)DO UPDATE SET rev=excluded.rev,actor=excluded.actor,deleted=excluded.deleted,seq=excluded.seq",params![change.t,key,change.rev,change.actor,change.deleted]).map_err(err)?;
        applied += 1;
    }
    tx.execute_batch("UPDATE peer_control SET muted=0;")
        .map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(applied)
}
pub fn replace_from_stage(c: &mut Connection) -> Result<usize, String> {
    replace_with_state(c, &[])
}
pub fn replace_with_state(c: &mut Connection, updates: &[(&str, String)]) -> Result<usize, String> {
    {
        let mut stmt = c
            .prepare("SELECT DISTINCT json_extract(body,'$.t') FROM peer_stage")
            .map_err(err)?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        if names.iter().any(|t| !TABLES.contains(&t.as_str())) {
            return Err("Недопустимая таблица переноса".into());
        }
    }

    // One transaction: a bad row/FK/schema leaves the previous Android history intact.
    c.execute_batch(
        "BEGIN IMMEDIATE;PRAGMA defer_foreign_keys=ON;UPDATE peer_control SET muted=1;",
    )
    .map_err(err)?;
    let result = (|| {
        for &t in TABLES.iter().rev() {
            let where_sql = if t == "settings" {
                format!(" WHERE {}", local_filter(""))
            } else {
                String::new()
            };
            c.execute_batch(&format!("DELETE FROM \"{t}\"{where_sql};"))
                .map_err(err)?;
        }
        c.execute("DELETE FROM peer_rows", []).map_err(err)?;
        // Parents are inserted before children while the outer transaction stays atomic.
        Ok::<_, String>(())
    })();
    if let Err(e) = result {
        let _ = c.execute_batch("ROLLBACK;");
        return Err(e);
    }
    // Caller uses apply_staged inside the same outer transaction (below).
    let result = (|| {
        let n = apply_staged(c)?;
        set_state(c, "sent", &tick(c)?.to_string())?;
        for (key, value) in updates {
            set_state(c, key, value)?;
        }
        Ok::<usize, String>(n)
    })();
    if result.is_ok() {
        if let Err(e) = c.execute_batch("UPDATE peer_control SET muted=0;COMMIT;") {
            let _ = c.execute_batch("ROLLBACK;");
            return Err(err(e));
        }
    } else {
        let _ = c.execute_batch("ROLLBACK;");
    }
    result
}
fn apply_staged(c: &Connection) -> Result<usize, String> {
    let mut count = 0;
    for &t in TABLES {
        let mut stmt = c
            .prepare("SELECT body FROM peer_stage WHERE json_extract(body,'$.t')=?1 ORDER BY n")
            .map_err(err)?;
        let mut rows = stmt.query([t]).map_err(err)?;
        while let Some(row) = rows.next().map_err(err)? {
            let change: Change =
                serde_json::from_str(&row.get::<_, String>(0).map_err(err)?).map_err(err)?;
            if change.deleted {
                continue;
            }
            let cols = columns(c, t)?;
            let row = change
                .row
                .as_ref()
                .and_then(Value::as_object)
                .ok_or("Нет записи")?;
            if row.len() != cols.len() || cols.iter().any(|x| !row.contains_key(&x.name)) {
                return Err("Несовместимая структура базы".into());
            }
            if t == "settings"
                && !change
                    .k
                    .first()
                    .and_then(Value::as_str)
                    .is_some_and(shared_setting)
            {
                return Err("Недопустимая настройка".into());
            }
            if pks(&cols).len() != change.k.len()
                || change.actor.len() != 32
                || !change.actor.bytes().all(|b| b.is_ascii_hexdigit())
                || change.rev < 1
                || change.rev > 9_000_000_000_000_000
            {
                return Err("Неверная версия или ключ записи".into());
            }
            for (col, v) in pks(&cols).iter().zip(&change.k) {
                if row.get(&col.name) != Some(v) {
                    return Err("Неверный ключ".into());
                }
            }
            let fields = cols
                .iter()
                .map(|x| format!("\"{}\"", x.name))
                .collect::<Vec<_>>()
                .join(",");
            c.execute(
                &format!(
                    "INSERT INTO \"{t}\"({fields})VALUES({})",
                    vec!["?"; cols.len()].join(",")
                ),
                params_from_iter(
                    cols.iter()
                        .map(|x| sqlval(&row[&x.name]))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            )
            .map_err(err)?;
            c.execute("UPDATE peer_control SET tick=max(tick,?1)+1", [change.rev])
                .map_err(err)?;
            c.execute("INSERT INTO peer_rows(t,k,rev,actor,deleted,seq)SELECT ?1,?2,?3,?4,0,tick FROM peer_control",params![t,serde_json::to_string(&change.k).map_err(err)?,change.rev,change.actor]).map_err(err)?;
            count += 1;
        }
    }
    Ok(count)
}
pub fn counts(c: &Connection) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for &t in TABLES {
        let n: i64 = c
            .query_row(&format!("SELECT count(*)FROM \"{t}\""), [], |r| r.get(0))
            .map_err(err)?;
        out.insert(t.into(), json!(n));
    }
    Ok(Value::Object(out))
}
