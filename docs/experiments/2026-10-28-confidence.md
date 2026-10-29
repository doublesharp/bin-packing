# Library confidence follow-up

Subsequent [Terra Low batches](2026-10-28-terra-test-batches.md) record additional
tests and the latest production coverage measurement.

Added 41 Rust regression/property tests and six tests for the reusable coverage
reporter. The Rust suite now passes 577 tests; the Python tooling suite passes
43. One pre-existing optional benchmark remains ignored.

Production library coverage is **95.96% of lines** (15,213/15,853) and **83.51%
of branch outcomes** (3,130/3,748). The counting scope is unchanged: unique LLVM
LCOV source-line and branch records, excluding only test code. The new reporter
was checked against the earlier baseline and reproduced it exactly. The current
measurement passes explicit minimums of 95% lines and 82% branches.

## Correctness evidence

New irregular geometry tests reproduced a strict-containment violation: a hole
touching its polygon's exterior at exactly one point was accepted. The fix rejects
zero-distance exterior/hole contact after topology validation. Point contact and
shared edges reject; positive gaps of 1e-6 and 0.25 remain accepted for both stock
and demand shapes. Existing curved uncertainty rules are preserved. The failing
test was observed before the production change and passed afterward.

The added tests also check:

- 324 independently calculated exact-circle clearance/containment cases and
  analytic lens, capsule, annular-sector and full-circle areas.
- 72 physical strip tilings and 54 stricter cut-limit rejections, including kerf
  conservation, source accounting and corrupted cut trees.
- Hand-calculated axle reactions on both floor axes, signed reactions and rounding,
  exact density/payload/fragility/count limits, nesting and forged certificates.
- Mandatory/unfit demand accounting, invalid controls, stock limits, cost versus
  count objectives, trimmed/defective crops and fixed supports after cropping.
- 64 fixed-seed generated 3D jobs across 28 separate strategies: 1,792 solver
  calls checking nonempty results, finite inventory, the single allowed rotation,
  independent source-axis permutations, bounds, overlap and quantity conservation.

The portable package contract now checks the strict-hole regression through
native Node and WebAssembly APIs, including actual Chromium execution.

## Verification and reproducibility

`./scripts/check-quality.sh` passes all Rust/Python tests, rustfmt, Taplo, typos,
strict all-target/all-feature Clippy and rustdoc. `./scripts/check-bindings.sh`
rebuilds and verifies the native package and all 15 WASM targets, 16 declaration contracts,
300 generated cases/1,800 solver calls and five actual Chromium variants.
Isolated Rust/WASM feature checks, fuzz-target formatting/Clippy, both doctests
and Cargo package verification also pass. No package was published.

An independent scoped review found a reporting hazard in the new utility:
literal/comment text could be mistaken for a test-only attribute. A regression
failed before the fix; lexical marker detection now preserves all production
lines. All six reporter tests pass, and the scoped re-review has no open material
findings. Cargo JSON supplies exact artifact paths, avoiding assumptions about
nightly Cargo's directory layout.

See [the testing guide](../testing-confidence.md) for commands and proof scope.
Coverage uses nightly-2026-10-12 and its matching LLVM tools on macOS ARM;
the project toolchain remains unchanged. The 577 tests also pass under coverage
instrumentation, with unchanged source hashes. JavaScript/browser line coverage
and doctests are outside the Rust coverage percentage.

Raw profiles, counters, source hashes, missing lines/branches, commands and gate
logs remain under `/Volumes/Scratch/bin-packing/reports/confidence-20261028/`.
The earlier baseline remains under `reports/coverage-20261028/`. No raw evidence
was added to git. Disposable instrumented builds and downloaded test tools are
removed after ownership/open-file checks; the shared warm build/cache remains.
