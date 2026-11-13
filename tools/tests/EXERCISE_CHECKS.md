# Exercise library checks

See `docs/EXERCISE_LIBRARY_1_7.md` for scope and limitations.

```
python tools/exercises/validate_catalog.py
cargo test --manifest-path tools/exercise-tests/Cargo.toml -- --test-threads=1
npm run build
# Start npm run dev in a second terminal, then:
python tools/tests/exercise_library.py --url http://127.0.0.1:5173 --output /tmp/kouros-library-check
```

The browser suite uses fictional demo data, checks search/favorites/custom media/editing, selects exercises in strength and OFP, and verifies original complex names/media. It writes screenshots and a JSON report. Python Playwright and Chromium must be installed. Full Windows native checking and a physical installer upgrade test are separate release checks.
