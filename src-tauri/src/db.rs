//! Encrypted SQLite (SQLCipher) database, schema migrations and backups.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const DB_FILE: &str = "kouros.db";
const KEEP_BACKUPS: usize = 14;
const AUTO_BACKUP_EVERY: Duration = Duration::from_secs(20 * 3600);

/// Each entry upgrades the schema by one version. Never edit a shipped entry —
/// append a new one instead, so existing databases upgrade in place.
const MIGRATIONS: &[&str] = &[
    // v1 — base tables
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE sources (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        connected  INTEGER NOT NULL DEFAULT 0,
        last_sync  TEXT,
        last_error TEXT
    );
    INSERT INTO sources (id, name) VALUES
        ('strava',    'Strava'),
        ('freddy',    'Apple Health (freddy)'),
        ('files',     'Файлы тренировок'),
        ('anthropic', 'Claude API');
    CREATE TABLE app_log (
        id      INTEGER PRIMARY KEY AUTOINCREMENT,
        at      TEXT NOT NULL DEFAULT (datetime('now')),
        kind    TEXT NOT NULL,
        message TEXT NOT NULL
    );
    INSERT INTO app_log (kind, message) VALUES ('init', 'База данных создана');
    "#,
    // v2 — workouts, health metrics, imports, sync state
    r#"
    CREATE TABLE activities (
        id               TEXT PRIMARY KEY,
        strava_id        INTEGER UNIQUE,
        sport            TEXT NOT NULL,
        sport_label      TEXT,
        name             TEXT,
        description      TEXT,
        start_utc        INTEGER NOT NULL,
        elapsed_s        REAL,
        moving_s         REAL,
        distance_m       REAL,
        elev_gain_m      REAL,
        avg_hr           REAL,
        max_hr           REAL,
        avg_cadence      REAL,
        avg_power        REAL,
        calories         REAL,
        relative_effort  REAL,
        steps            INTEGER,
        total_sets       INTEGER,
        total_reps       INTEGER,
        weight_lifted_kg REAL,
        gear             TEXT,
        device           TEXT,
        source           TEXT NOT NULL,
        file_path        TEXT,
        has_streams      INTEGER NOT NULL DEFAULT 0,
        has_gps          INTEGER NOT NULL DEFAULT 0,
        created_at       TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at       TEXT
    );
    CREATE INDEX activities_start ON activities (start_utc);
    CREATE TABLE activity_streams (
        activity_id TEXT PRIMARY KEY REFERENCES activities (id) ON DELETE CASCADE,
        data        TEXT NOT NULL
    );
    CREATE TABLE strength_sets (
        activity_id TEXT NOT NULL REFERENCES activities (id) ON DELETE CASCADE,
        idx         INTEGER NOT NULL,
        start_utc   INTEGER,
        duration_s  REAL,
        reps        INTEGER,
        weight_kg   REAL,
        kind        TEXT,
        category    INTEGER,
        PRIMARY KEY (activity_id, idx)
    );
    CREATE TABLE import_log (
        id      INTEGER PRIMARY KEY AUTOINCREMENT,
        at      TEXT NOT NULL DEFAULT (datetime('now')),
        title   TEXT NOT NULL,
        kind    TEXT NOT NULL,
        found   INTEGER NOT NULL DEFAULT 0,
        added   INTEGER NOT NULL DEFAULT 0,
        merged  INTEGER NOT NULL DEFAULT 0,
        skipped INTEGER NOT NULL DEFAULT 0,
        errors  INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE health_metrics (
        metric    TEXT NOT NULL,
        date      TEXT NOT NULL,
        ts        TEXT NOT NULL DEFAULT '',
        provider  TEXT NOT NULL DEFAULT '',
        device    TEXT NOT NULL DEFAULT '',
        source_id TEXT NOT NULL DEFAULT '',
        source    TEXT,
        num       REAL,
        txt       TEXT,
        unit      TEXT,
        PRIMARY KEY (metric, date, ts, provider, device, source_id)
    );
    CREATE INDEX health_metric_date ON health_metrics (metric, date);
    CREATE TABLE sync_state (
        source     TEXT PRIMARY KEY,
        cursor     TEXT,
        last_ok    TEXT,
        last_error TEXT,
        extra      TEXT
    );
    "#,
    // v3 — engine cache: per-workout load, zones and best efforts
    r#"
    CREATE TABLE activity_metrics (
        activity_id TEXT PRIMARY KEY REFERENCES activities (id) ON DELETE CASCADE,
        version     TEXT NOT NULL,
        load        REAL NOT NULL,
        method      TEXT NOT NULL,
        zone_s      TEXT NOT NULL,
        best        TEXT NOT NULL
    );
    "#,
    // v4 — perceived effort, notes, exercise library for strength sessions
    r#"
    ALTER TABLE activities ADD COLUMN rpe REAL;
    ALTER TABLE activities ADD COLUMN notes TEXT;
    ALTER TABLE strength_sets ADD COLUMN exercise TEXT;
    ALTER TABLE strength_sets ADD COLUMN exercise_id INTEGER;
    CREATE TABLE exercises (
        id                INTEGER PRIMARY KEY AUTOINCREMENT,
        name              TEXT NOT NULL UNIQUE,
        category          TEXT,
        muscles_primary   TEXT,
        muscles_secondary TEXT,
        equipment         TEXT,
        image_path        TEXT,
        source            TEXT NOT NULL DEFAULT 'manual',
        created_at        TEXT NOT NULL DEFAULT (datetime('now'))
    );
    "#,
    // v5 — shoes per workout, exercise library with muscles
    r#"
    ALTER TABLE activities ADD COLUMN shoe_id TEXT;
    INSERT INTO exercises (name, category, muscles_primary, muscles_secondary, equipment, source) VALUES
        ('Жим штанги лёжа', 'strength', 'chest', 'triceps,shoulders', 'штанга', 'library'),
        ('Жим гантелей лёжа', 'strength', 'chest', 'triceps,shoulders', 'гантели', 'library'),
        ('Жим гантелей сидя', 'strength', 'shoulders', 'triceps', 'гантели', 'library'),
        ('Отжимания', 'strength', 'chest', 'triceps,shoulders,abs', 'свой вес', 'library'),
        ('Отжимания на брусьях', 'strength', 'chest,triceps', 'shoulders', 'свой вес', 'library'),
        ('Подтягивания', 'strength', 'lats', 'biceps,forearms', 'свой вес', 'library'),
        ('Тяга верхнего блока', 'strength', 'lats', 'biceps', 'тренажёр', 'library'),
        ('Тяга горизонтального блока', 'strength', 'lats', 'biceps,traps', 'тренажёр', 'library'),
        ('Тяга гантели в наклоне', 'strength', 'lats', 'biceps,traps', 'гантели', 'library'),
        ('Становая тяга', 'strength', 'hamstrings,glutes,lowerback', 'quads,traps,forearms', 'штанга', 'library'),
        ('Румынская тяга', 'strength', 'hamstrings,glutes', 'lowerback', 'штанга', 'library'),
        ('Приседания со штангой', 'strength', 'quads,glutes', 'hamstrings,lowerback,abs', 'штанга', 'library'),
        ('Гоблет-присед', 'strength', 'quads,glutes', 'abs', 'гантели', 'library'),
        ('Выпады', 'strength', 'quads,glutes', 'hamstrings,calves', 'гантели', 'library'),
        ('Болгарские выпады', 'strength', 'quads,glutes', 'hamstrings', 'гантели', 'library'),
        ('Жим ногами', 'strength', 'quads,glutes', 'hamstrings', 'тренажёр', 'library'),
        ('Разгибание ног', 'strength', 'quads', '', 'тренажёр', 'library'),
        ('Сгибание ног', 'strength', 'hamstrings', 'calves', 'тренажёр', 'library'),
        ('Подъём на носки', 'strength', 'calves', '', 'тренажёр', 'library'),
        ('Ягодичный мост', 'strength', 'glutes', 'hamstrings', 'свой вес', 'library'),
        ('Махи гантелями в стороны', 'strength', 'shoulders', 'traps', 'гантели', 'library'),
        ('Сгибание рук со штангой', 'strength', 'biceps', 'forearms', 'штанга', 'library'),
        ('Французский жим', 'strength', 'triceps', '', 'штанга', 'library'),
        ('Планка', 'strength', 'abs', 'shoulders,lowerback', 'свой вес', 'library'),
        ('Скручивания', 'strength', 'abs', '', 'свой вес', 'library'),
        ('Гиперэкстензия', 'strength', 'lowerback', 'glutes,hamstrings', 'тренажёр', 'library'),
        ('Упражнение с резинкой', 'strength', '', '', 'резинка', 'library'),
        ('Растяжка', 'strength', '', '', 'свой вес', 'library')
    ON CONFLICT(name) DO UPDATE SET
        muscles_primary = COALESCE(exercises.muscles_primary, excluded.muscles_primary),
        muscles_secondary = COALESCE(exercises.muscles_secondary, excluded.muscles_secondary),
        equipment = COALESCE(exercises.equipment, excluded.equipment),
        category = COALESCE(exercises.category, excluded.category);
    "#,
    // v6 — habit diary (one mark per day and habit)
    r#"
    CREATE TABLE habit_log (
        date TEXT NOT NULL,
        key  TEXT NOT NULL,
        val  INTEGER NOT NULL,
        PRIMARY KEY (date, key)
    );
    "#,
    // v7 — questionnaires (morning check-in, readiness before training, mood profile, stress and recovery)
    r#"
    CREATE TABLE surveys (
        id      INTEGER PRIMARY KEY AUTOINCREMENT,
        kind    TEXT NOT NULL,
        date    TEXT NOT NULL,
        ts      INTEGER NOT NULL,
        answers TEXT NOT NULL,
        score   REAL,
        extra   TEXT
    );
    CREATE INDEX surveys_kind_date ON surveys (kind, date);
    "#,
    // v8 — morning readiness, frozen once the night is in (later daytime readings must not rewrite it)
    r#"
    CREATE TABLE readiness_log (
        date      TEXT PRIMARY KEY,
        score     REAL,
        level     TEXT NOT NULL,
        data      TEXT NOT NULL,
        frozen_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    "#,
    // v9 — evening readiness, last value of the evening (from 17:30) kept per day
    r#"
    CREATE TABLE evening_log (
        date     TEXT PRIMARY KEY,
        score    REAL NOT NULL,
        level    TEXT NOT NULL,
        data     TEXT NOT NULL,
        saved_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    "#,
    // v10 — muscle load per workout (by muscle group) and its recovery time
    r#"
    CREATE TABLE muscle_cache (
        activity_id TEXT PRIMARY KEY REFERENCES activities (id) ON DELETE CASCADE,
        version     TEXT NOT NULL,
        data        TEXT NOT NULL
    );
    "#,
    // v11 — «Витамины и лекарства»: your preparations (composition from the label) and intake marked by hand
    r#"
    CREATE TABLE meds (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        kind        TEXT NOT NULL,
        name        TEXT NOT NULL,
        form        TEXT,
        dose        REAL NOT NULL DEFAULT 1,
        schedule    TEXT NOT NULL DEFAULT 'daily',
        components  TEXT NOT NULL DEFAULT '[]',
        course_from TEXT,
        course_to   TEXT,
        notes       TEXT,
        archived    INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE med_log (
        id       INTEGER PRIMARY KEY AUTOINCREMENT,
        med_id   INTEGER NOT NULL REFERENCES meds (id) ON DELETE CASCADE,
        date     TEXT NOT NULL,
        ts       INTEGER NOT NULL,
        servings REAL NOT NULL DEFAULT 1
    );
    CREATE INDEX med_log_date ON med_log (date);
    "#,
    // v12: coach reports (day / week / Claude answers)
    r#"
    CREATE TABLE coach_reports (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        kind        TEXT NOT NULL,
        ts          INTEGER NOT NULL,
        period_from TEXT,
        period_to   TEXT,
        title       TEXT NOT NULL,
        body        TEXT NOT NULL,
        data        TEXT,
        parent_id   INTEGER
    );
    CREATE INDEX coach_reports_ts ON coach_reports (kind, ts);
    "#,
    // v13: training plan days and the history of plan changes
    r#"
    CREATE TABLE plan_days (
        date   TEXT PRIMARY KEY,
        items  TEXT NOT NULL,
        source TEXT NOT NULL,
        note   TEXT,
        ts     INTEGER NOT NULL
    );
    CREATE TABLE plan_changes (
        id     INTEGER PRIMARY KEY AUTOINCREMENT,
        ts     INTEGER NOT NULL,
        date   TEXT NOT NULL,
        before TEXT NOT NULL,
        after  TEXT NOT NULL,
        reason TEXT NOT NULL,
        origin TEXT NOT NULL,
        state  TEXT NOT NULL,
        grp    INTEGER
    );
    CREATE INDEX plan_changes_date ON plan_changes (date);
    "#,
    // v14: post-workout check-ins and the pain journal
    r#"
    CREATE TABLE checkins (
        activity_id TEXT PRIMARY KEY,
        ts          INTEGER NOT NULL,
        rpe         INTEGER,
        feel        INTEGER,
        pain        TEXT,
        note        TEXT,
        skipped     INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE pain_log (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        date        TEXT NOT NULL,
        ts          INTEGER NOT NULL,
        area        TEXT NOT NULL,
        side        TEXT NOT NULL DEFAULT '',
        level       INTEGER NOT NULL,
        context     TEXT NOT NULL,
        activity_id TEXT,
        note        TEXT
    );
    CREATE INDEX pain_log_date ON pain_log (date);
    "#,
    // v15: workout sources — hidden state, duplicate groups, deletion
    r#"
    ALTER TABLE activities ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE activities ADD COLUMN dup_group TEXT;
    ALTER TABLE activities ADD COLUMN dup_state TEXT;
    ALTER TABLE activities ADD COLUMN deleted_ts INTEGER;
    CREATE INDEX activities_hidden ON activities (hidden, start_utc);
    "#,
    // v16: vitamins & medicines 2 — product details (catalog source, nutrition toggle, group) and
    // the medicine registry downloaded by «Обновить реестр»
    r#"
    ALTER TABLE meds ADD COLUMN extra TEXT;
    CREATE TABLE drug_registry (
        trade TEXT NOT NULL,
        inn   TEXT NOT NULL,
        form  TEXT,
        dose  TEXT
    );
    CREATE INDEX drug_registry_trade ON drug_registry (trade);
    "#,
    // v17: lab results — reports (PDF read on this computer or Claude's [АНАЛИЗ] lines) and rows
    r#"
    CREATE TABLE lab_reports (
        id         INTEGER PRIMARY KEY,
        date       TEXT NOT NULL,
        lab        TEXT,
        file       TEXT,
        source     TEXT,
        created_at INTEGER,
        note       TEXT
    );
    CREATE TABLE lab_results (
        id         INTEGER PRIMARY KEY,
        report_id  INTEGER NOT NULL REFERENCES lab_reports (id) ON DELETE CASCADE,
        date       TEXT NOT NULL,
        key        TEXT,
        name       TEXT NOT NULL,
        value      REAL,
        value_text TEXT,
        cmp        TEXT,
        flag       TEXT,
        unit       TEXT,
        lo         REAL,
        hi         REAL,
        ref_text   TEXT,
        section    TEXT,
        comment    TEXT
    );
    CREATE INDEX lab_results_key ON lab_results (key, date);
    "#,
    // v18: exercise library — catalog id, resistance bands and the effective load of a set
    r#"
    ALTER TABLE strength_sets ADD COLUMN ex_id TEXT;
    ALTER TABLE strength_sets ADD COLUMN band TEXT;
    ALTER TABLE strength_sets ADD COLUMN eff_kg REAL;
    "#,
    // v19: the athlete confirmed the day's plan (0.23)
    r#"
    ALTER TABLE plan_days ADD COLUMN confirmed INTEGER;
    "#,
];

fn key_pragma(hex: &str) -> String {
    format!("PRAGMA key = \"x'{hex}'\";")
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Opens (or creates) the encrypted database in `root` and brings the schema up to date.
pub fn open(root: &Path) -> Result<Connection, String> {
    let path = root.join(DB_FILE);
    let is_new = !path.exists();
    let hex = crate::secrets::db_key(is_new)?;
    let conn = Connection::open(&path).map_err(err)?;
    conn.execute_batch(&key_pragma(&hex)).map_err(err)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map_err(|_| {
            "Не удалось открыть базу: ключ не подходит. Нужен ключ восстановления.".to_string()
        })?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(err)?;
    migrate(&conn)?;
    // Every reader goes through the view of the chosen data sources (0.14).
    crate::sources::install_view(&conn)?;
    crate::workouts_svc::install_view(&conn)?;
    Ok(conn)
}

pub fn schema_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    let current = schema_version(conn) as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = i + 1;
        let batch = format!("BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;");
        conn.execute_batch(&batch)
            .map_err(|e| format!("Ошибка обновления базы до версии {version}: {e}"))?;
    }
    Ok(())
}

pub fn cipher_version(conn: &Connection) -> Option<String> {
    conn.query_row("PRAGMA cipher_version", [], |r| r.get::<_, String>(0)).ok()
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map(|_| ())
    .map_err(err)
}

pub fn log(conn: &Connection, kind: &str, message: &str) {
    let _ = conn.execute(
        "INSERT INTO app_log (kind, message) VALUES (?1, ?2)",
        params![kind, message],
    );
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
}

pub fn sources(conn: &Connection) -> Result<Vec<Source>, String> {
    let mut st = conn
        .prepare("SELECT id, name, connected, last_sync, last_error FROM sources ORDER BY rowid")
        .map_err(err)?;
    let rows = st
        .query_map([], |r| {
            Ok(Source {
                id: r.get(0)?,
                name: r.get(1)?,
                connected: r.get::<_, i64>(2)? != 0,
                last_sync: r.get(3)?,
                last_error: r.get(4)?,
            })
        })
        .map_err(err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(err)
}

pub fn size_bytes(root: &Path) -> u64 {
    ["", "-wal", "-shm"]
        .iter()
        .filter_map(|s| std::fs::metadata(root.join(format!("{DB_FILE}{s}"))).ok())
        .map(|m| m.len())
        .sum()
}

// ---------- backups ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: u64,
}

fn backup_dir(root: &Path) -> PathBuf {
    root.join("backups")
}

pub fn list_backups(root: &Path) -> Vec<BackupInfo> {
    let mut out: Vec<BackupInfo> = std::fs::read_dir(backup_dir(root))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with("kouros-") && name.ends_with(".db")) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some(BackupInfo { name, size: meta.len(), modified })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Copies the live database into backups/ with the same encryption key.
pub fn backup_now(conn: &Connection, root: &Path) -> Result<String, String> {
    let hex = crate::secrets::db_key(false)?;
    let name = format!("kouros-{}.db", chrono::Local::now().format("%Y-%m-%d_%H%M%S"));
    let path = backup_dir(root).join(&name);
    std::fs::create_dir_all(backup_dir(root)).map_err(err)?;
    {
        let mut dst = Connection::open(&path).map_err(err)?;
        dst.execute_batch(&key_pragma(&hex)).map_err(err)?;
        let b = rusqlite::backup::Backup::new(conn, &mut dst).map_err(err)?;
        b.run_to_completion(256, Duration::from_millis(5), None)
            .map_err(|e| format!("Резервная копия не создана: {e}"))?;
    }
    prune(root);
    log(conn, "backup", &format!("Резервная копия {name}"));
    Ok(name)
}

fn prune(root: &Path) {
    for old in list_backups(root).into_iter().skip(KEEP_BACKUPS) {
        let _ = std::fs::remove_file(backup_dir(root).join(old.name));
    }
}

/// Makes a backup if the newest one is older than ~20 hours.
pub fn auto_backup(conn: &Connection, root: &Path) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let fresh = list_backups(root)
        .first()
        .map(|b| now.saturating_sub(b.modified) < AUTO_BACKUP_EVERY.as_secs())
        .unwrap_or(false);
    if !fresh {
        backup_now(conn, root)?;
    }
    Ok(())
}
