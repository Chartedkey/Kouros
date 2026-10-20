# Kouros — notes for Claude

Desktop app for runners (Windows): training, sleep, HRV, nutrition, supplements and a rule-based coach. UI language: Russian.

## Before any work
- This repository is **public**. The full project notes and the design document live in the private repository `Chartedkey/Kouros-private` (`CLAUDE.md`, `docs/KOUROS.md`). Attach it (add_repo) and read both before changing anything; keep them up to date there, never here.
- Never commit personal data to this repository: no names, body measurements, test results, health numbers, real exports or screenshots with data, keys or tokens.
- Never start building a new version until the owner explicitly confirms («давай», «начинай», «делай»).

## Stack (short)
- `src/` React 18 + TS + Vite; `src/lib/api.ts` wraps Tauri commands and has a browser demo backend.
- `src-tauri/` Tauri 2 + Rust; SQLCipher DB; migrations appended to `MIGRATIONS` in `db.rs` (never edit a shipped one). Readers use the TEMP VIEWs `hm` (health) and `va` (counted workouts).
- `src-tauri/src/engine/` std-only formulas; `*_svc.rs` services build the JSON for each screen.
- Catalog of supplements / medicine registry: `tools/catalog/build.py` via `.github/workflows/catalog.yml` → `src-tauri/resources/*.json.gz`.
- CI: `check.yml` on branch pushes (tsc, vite build, cargo check; Rust errors as annotations), `build.yml` on `main` builds the NSIS installer and publishes release `v<version>`. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` together and add a `## <version>` section to `CHANGELOG.md` (it becomes the release description).
- Demo data in `src/lib/api.ts` is fictional and must stay so.
