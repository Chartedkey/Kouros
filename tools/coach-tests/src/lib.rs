#![allow(dead_code)]
#[path = "../../../src-tauri/src/activity_svc.rs"]
pub mod activity_svc;
#[path = "../../../src-tauri/src/ai.rs"]
pub mod ai;
#[path = "../../../src-tauri/src/ai_context.rs"]
pub mod ai_context;
#[path = "../../../src-tauri/src/body_svc.rs"]
pub mod body_svc;
#[path = "../../../src-tauri/src/catalog_svc.rs"]
pub mod catalog_svc;
#[path = "../../../src-tauri/src/coach_svc.rs"]
pub mod coach_svc;
#[path = "../../../src-tauri/src/drugs.rs"]
pub mod drugs;
#[path = "../../../src-tauri/src/engine/mod.rs"]
pub mod engine;
#[path = "../../../src-tauri/src/engine_svc.rs"]
pub mod engine_svc;
#[path = "../../../src-tauri/src/exercise_svc.rs"]
pub mod exercise_svc;
#[path = "../../../src-tauri/src/food_dishes.rs"]
pub mod food_dishes;
#[path = "../../../src-tauri/src/food_est.rs"]
pub mod food_est;
#[path = "../../../src-tauri/src/health_svc.rs"]
pub mod health_svc;
#[path = "../../../src-tauri/src/labs_svc.rs"]
pub mod labs_svc;
#[path = "../../../src-tauri/src/manual_svc.rs"]
pub mod manual_svc;
#[path = "../../../src-tauri/src/meds_svc.rs"]
pub mod meds_svc;
#[path = "../../../src-tauri/src/muscle_svc.rs"]
pub mod muscle_svc;
#[path = "../../../src-tauri/src/norms.rs"]
pub mod norms;
#[path = "../../../src-tauri/src/nutrition_svc.rs"]
pub mod nutrition_svc;
#[path = "../../../src-tauri/src/parse/mod.rs"]
pub mod parse;
#[path = "../../../src-tauri/src/peer_data.rs"]
pub mod peer_data;
#[path = "../../../src-tauri/src/plan_svc.rs"]
pub mod plan_svc;
#[path = "../../../src-tauri/src/planning.rs"]
pub mod planning;
#[path = "../../../src-tauri/src/psych_svc.rs"]
pub mod psych_svc;
#[path = "../../../src-tauri/src/sources.rs"]
pub mod sources;
#[path = "../../../src-tauri/src/store.rs"]
pub mod store;
#[path = "../../../src-tauri/src/stress_svc.rs"]
pub mod stress_svc;
#[path = "../../../src-tauri/src/workouts_svc.rs"]
pub mod workouts_svc;
pub mod garmin {
    pub const GARMIN: &str = "Garmin";
}
pub mod db {
    use rusqlite::{params, Connection, OptionalExtension};
    fn err(e: impl std::fmt::Display) -> String {
        e.to_string()
    }
    include!(concat!(env!("OUT_DIR"), "/db.rs"));
    pub fn migrate_fixture(c: &Connection) {
        migrate(c).unwrap();
    }
}
#[cfg(test)]
mod tests;
