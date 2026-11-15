#![allow(dead_code)]
#[path = "../../../src-tauri/src/engine/mod.rs"]
pub mod engine;
#[path = "../../../src-tauri/src/exercise_svc.rs"]
pub mod exercise_svc;
#[path = "../../../src-tauri/src/muscle_svc.rs"]
pub mod muscle_svc;
#[path = "../../../src-tauri/src/parse/mod.rs"]
pub mod parse;
#[path = "../../../src-tauri/src/store.rs"]
pub mod store;

#[path = "../../../src-tauri/src/ai.rs"]
pub mod ai;
#[path = "../../../src-tauri/src/planning.rs"]
pub mod planning;
pub mod db {
    use rusqlite::{params, Connection, OptionalExtension};
    fn err<E: std::fmt::Display>(e: E) -> String {
        e.to_string()
    }
    include!(concat!(env!("OUT_DIR"), "/db.rs"));
    pub fn migrate_fixture(c: &Connection) {
        migrate(c).unwrap();
    }
    pub fn migrate_to_previous(c: &Connection) {
        for (i, sql) in MIGRATIONS.iter().enumerate().take(23) {
            c.execute_batch(&format!(
                "BEGIN;\n{sql}\nPRAGMA user_version = {};\nCOMMIT;",
                i + 1
            ))
            .unwrap();
        }
    }
}
pub mod engine_svc {
    use crate::{
        engine::{
            training::{self, ActInput},
            Profile,
        },
        store,
    };
    use rusqlite::{params, Connection};
    use serde_json::{json, Value};
    fn err<E: std::fmt::Display>(e: E) -> String {
        e.to_string()
    }
    include!(concat!(env!("OUT_DIR"), "/metrics.rs"));
    // Only used by exercise display helpers, not the calculation under test.
    pub fn today(p: &Profile) -> String {
        crate::engine::day_of(crate::store::now(), p.tz_offset_s)
    }
    pub fn profile(_: &Connection) -> Profile {
        Profile::default()
    }
}
pub mod activity_svc {
    include!(concat!(env!("OUT_DIR"), "/sides.rs"));
}

#[cfg(test)]
mod intensity_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod ai_tests;
