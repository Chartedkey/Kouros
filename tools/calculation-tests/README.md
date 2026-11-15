# Calculation regression tests

Run `cargo test --manifest-path tools/calculation-tests/Cargo.toml -- --test-threads=1`.

This crate compiles the production engine, parsers, store, muscle and exercise services without Tauri. The build script includes the actual schema migrations and the current metrics-cache functions verbatim. There are no copied calculation formulas. The default-profile helper only satisfies exercise display dependencies; tests pass their calculation profile explicitly. Fixtures use SQLite in memory and never open the personal database.

Coverage includes partial/sparse HR, real gaps and pauses, malformed samples, RPE, late/changed/deleted/identical streams, data updates through direct SQL (device sync), transaction rollback, old engine cache rebuilding, the previous schema upgrade and preservation of user data, and history-window boundaries.

Policy for this fix:

- A pulse curve requires at least 10 samples, strictly increasing nonnegative timestamps, plausible finite HR and at least 95% duration coverage through intervals of at most 30 seconds. Integrated time must not exceed the session's moving duration (or elapsed duration when moving time is absent).
- The 95% limit is a conservative data-quality tolerance, not a physiological threshold. Unrecorded time and long gaps are never filled by extending a sample. Without sufficient coverage, use the existing average-HR method or sport/time estimate. RPE keeps its existing precedence for strength/workout/yoga. The existing method field records the chosen method.
- Sparse samples do not prove the duration of a pause. A continuous curve containing more time than the moving duration falls back rather than assigning pause HR to exercise. Proper timer-based segmentation, if added later, can recover this detail.
- Schema v24 invalidates derived workout caches when their input fields or stream JSON change, including direct SQL/device imports. Identical input deliveries and notes edits preserve caches. Existing workout results use engine revision e2, so e1 results rebuild on the next summary; personal records and frozen morning scores are preserved.
- Muscle ACWR requires an earliest record at least 34 days before the target day: 7 acute days and 28 preceding days. This checks the calendar boundary, not whether all sources were fully delivered inside it. Known rest and unreported activity still need richer import-coverage metadata in a future separately scoped change.

Package 2 adds a separate eligibility policy for automatic intensity advice:

- Both today's HRV and resting HR must be positive, finite, and present. Yesterday/future values are not substitutes.
- There must be at least 14 prior paired HRV/RHR days in the last 30 days, bounded by both current-source cut dates. Today's readings, non-overlapping histories, and invalid readings cannot fill the baseline.
- Today's sleep record must be present and at least 6 hours. Six hours is an operating cutoff to catch very short sleep, not a claim that six hours is sufficient for health or for this athlete.
- Any missing evidence or short sleep caps the computed recommendation at the existing calm tier (3). It can never raise a lower tier. These are conservative application policies, not validated physiological thresholds or proof of safe exercise.
- Morning and evening recommendations use the same independent limits. The weighted readiness score and its recorded history are unchanged; the evidence controls what advice can be generated from that score.
- With adequate evidence the previous score tiers and other checks still apply. TQR remains optional, with its existing reminder and existing effect when answered.
- Tests reproduce the two audit cases: HRV-only keeps a provisional score of 100, but the recommendation is calm; three-hour sleep keeps a weighted score of 70, but no intervals/tempo are recommended. Missing-data reasons remain visible in existing advice areas.

Readiness weights, ACWR formula variants and recovery coefficients are unchanged. No app version or release is created by this test crate.

## Recovery diagnostic (audit only)

`cargo run --manifest-path tools/calculation-tests/Cargo.toml --example recovery_audit`

This separately invoked example reports 15 groups of current recovery-model behaviours.
It calls production functions directly with synthetic data and prints JSON. Historical before-fix
evidence and the old diagnostic were archived with the audit. Acceptance criteria for corrected
deadlines and unknown data are in `src/recovery_tests.rs`; the reporting example is not run by
`cargo test`. No personal database, real API or new recovery coefficients are involved.

## Recovery timer contract (package 3)

- `recovery_at` takes an explicit clock. Fixed facts and readiness give fixed model deadlines.
  Readiness scales initial durations, and elapsed time is subtracted once. Sessions are grouped
  in deterministic 24-hour windows anchored at the latest completion; the existing 30% extra
  contribution is projected at that anchor, rather than repeatedly summed as a moving countdown.
- All history participates in grouping; old active horizons are not abruptly truncated after
  six days. Expired unknown groups are excluded only after a conservative bound from the
  existing 96-hour ceiling, 1.3 factor, and 30% policy. Zero-load rows cannot shift anchors.
- Sufficient baseline means calendar coverage back 42 days for CTL and 28 for muscle load
  before every relevant workout. This is a data-eligibility rule, not proof of complete imports
  or of physiological calibration. Unknown remains null and is never displayed as "ready".
- Future, ongoing and malformed recent workouts do not masquerade as completed load.
- The morning readiness factor is withheld if the independent evidence limits are nonempty.
- Positive residuals round up, so a positive remainder never rounds into a clearance.
- Cached per-workout estimates carry `recovery2`; legacy unqualified guesses are hidden.
- `node tools/calculation-tests/check_recovery_ui.mjs` verifies the actual tile's rendering
  of unknown, zero and positive estimates. It also checks that it does not announce physical
  readiness or invent an exact deadline from rounded hours.

The base recovery coefficients, the abrupt 24-hour grouping policy, and sensitivity to splitting
one workout across records remain calibration/model questions. The technical fix does not claim
to validate them or to solve fragmentation. No installer or release is generated by these checks.
