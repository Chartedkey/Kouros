use std::{fs, path::PathBuf};

// Compile service functions verbatim, without the Tauri/UI dependency graph.
// Changes to the extraction boundaries deliberately fail the build.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let tail = &text[text.find(start).expect(start)..];
    &tail[..tail.find(end).expect(end)]
}

fn main() {
    let root = PathBuf::from("../../src-tauri/src");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for file in ["db.rs", "engine_svc.rs", "activity_svc.rs"] {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    let db = fs::read_to_string(root.join("db.rs")).unwrap();
    let migrations = format!("{}\n];", between(&db, "const MIGRATIONS:", "\n];"));
    let migrate = between(&db, "pub fn schema_version(", "pub fn cipher_version(");
    let settings = between(&db, "pub fn get_setting(", "pub fn log(");
    fs::write(out.join("db.rs"), format!("{migrations}\n{migrate}\n{settings}")).unwrap();
    let svc = fs::read_to_string(root.join("engine_svc.rs")).unwrap();
    let version = between(&svc, "const ENGINE_V:", "\n");
    let metrics = between(&svc, "fn stream_pairs(", "// ---------- summary ----------");
    fs::write(out.join("metrics.rs"), format!("{version}\n{metrics}")).unwrap();
    let activity = fs::read_to_string(root.join("activity_svc.rs")).unwrap();
    fs::write(out.join("sides.rs"), between(&activity, "pub fn side_factor(", "/// Sets, reps and tonnage")).unwrap();
}
