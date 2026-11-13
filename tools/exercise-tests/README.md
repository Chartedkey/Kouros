# Exercise service regression tests

Compiles the production `src-tauri/src/exercise_svc.rs` directly, with minimal host adapters for settings/date access. SQLite databases are in memory; personal data and Tauri are not used.

```
cargo test --manifest-path tools/exercise-tests/Cargo.toml -- --test-threads=1
```

Use a single test thread because the production service has a process-wide CUSTOM cache. Tests cover preservation of all shipped IDs/coefficients, isolated custom saves, round trips through the old complexes editor, favorite persistence, and rejected input. Full native application compilation remains a separate check.
