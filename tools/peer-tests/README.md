# Replication tests

This small crate imports the actual `peer_data`, `peer_wire` and `peer_files`
modules without loading Tauri or touching the application's database or keychain.
Its schema is extracted from the shipped migration list at build time. All inputs
are fictional; temporary files live under the system temporary directory.

```
cargo test --manifest-path tools/peer-tests/Cargo.toml
```

Coverage includes initial transfer and foreign keys, atomic rollback, shared vs
local settings, offline integer IDs, concurrent edits, idempotent retries,
deletions and changed natural keys, bounded pagination, authenticated encryption,
wrong keys, response binding, real socket framing, malformed/oversized requests,
private addresses, attachment traversal, and hash verification before import.

The standalone crate uses bundled SQLite; release builds use SQLCipher. These
tests do not certify native keychain access, phone lifecycle, Windows Firewall,
installer behaviour, or a physical Windows-to-Android transfer.

UI checks use Vite and synthetic IPC:

```
npm run dev -- --host 127.0.0.1 --port 5175
python tools/tests/device_sync.py
python tools/tests/android_layout.py --screens Настройки
```
