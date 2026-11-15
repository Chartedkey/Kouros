# Coach integration checks

Runs the actual SQLite-backed planning, health, load, snapshot and account-response services without Tauri or personal data:

```sh
cargo test --manifest-path tools/coach-tests/Cargo.toml -- --test-threads=1
```

Uses the production migrations, source-selection views and replication setup in an in-memory database. Tests cover mode switching, preservation of accepted days, no competing local report or automatic/manual-choice rebalancing in DeepSeek mode, snapshot privacy and size, repeated snapshot identity, input invalidation and exclusion of AI/account settings from replication. Synthetic health and food records also exercise populated snapshots.

The neighbouring calculation-tests crate verifies the response schema, token/budget reservation, account-balance decoding, current restrictions, stale responses, cancellation, exercise validation and transactional plan acceptance. Tests never contact DeepSeek. An authenticated paid request must be checked with the user's own key in the installed app.
