# Correctness checks and production coverage

Run `./scripts/check-quality.sh` for the required Rust/Python test, formatting,
metadata, Clippy and documentation gate. `./scripts/check-features.sh` checks
isolated feature combinations; `./scripts/check-bindings.sh` builds and checks
the native and WebAssembly packages, declarations and actual browser entry points.

The tests include independent geometry, inventory and physical constraint checks:

- Analytic circular/arc areas and 324 exact-circle clearance cases, strict hole
  containment, source transforms and corrupted irregular certificates.
- 72 physical strip tilings, 54 stricter cut-limit rejections, cut-tree
  conservation and invalid source/tree references.
- Hand-calculated axle reactions, density/payload/stack boundaries, nested-floor
  bounds and forged loading metadata.
- Small finite knapsack oracles, mandatory demand accounting, objective changes,
  cropped defects/trims and fixed axle positions after cropping.
- A fixed-seed set of 64 generated jobs across 28 separate 3D strategies. Every
  result must place an item, respect finite stock and the single allowed source
  rotation, conserve quantities, stay within bounds and avoid overlap. Existing
  1D/2D property tests also use a fixed seed.

## Repeat the coverage measurement

Keep disposable output on Scratch. The coverage script uses the configured build
wrapper and a separate instrumented target directory. It needs the pinned nightly
toolchain and its `llvm-tools` component for unstable branch instrumentation;
the library's own toolchain declaration is unchanged.

```sh
python3 -B scripts/check-coverage.py \
  --output-dir /Volumes/Scratch/bin-packing/reports/coverage-new-run \
  --target-dir /Volumes/Scratch/bin-packing/build/coverage \
  --min-lines 96 --min-branches 90
```

Use a fresh output directory for each run. Target artifacts may be reused.
The script reads exact executable paths from Cargo JSON, runs all Rust test
targets with all features and exports profiles with the matching LLVM tools.
It records source hashes, commands, raw profiles, LLVM data, missing lines and
missing branch outcomes. It fails on test failure, missing data, source changes
or a requested minimum that is not met.

The reported percentages count unique LLVM LCOV source lines and branch outcomes
in the production Rust library. Test-only items/modules are excluded using Rust
lexical context, including literals and nested comments. Production code is not
excluded. This is the same counting scope as the initial October 28 measurement.
LLVM's per-function summary uses a different aggregation and remains available in
the raw export. JavaScript/browser execution and doctests are outside this Rust
coverage measurement.

Coverage identifies unexercised paths. Independent oracles and regression tests
provide the behavioral evidence; a percentage alone does not prove correctness.

The [October 28 confidence report](experiments/2026-10-28-confidence.md) records
the first expanded test set and validation repair. The subsequent
[Terra Low batches](experiments/2026-10-28-terra-test-batches.md) record the latest
additional test functions. The later
[existing-test and fault checks](experiments/2026-10-28-branch-proof.md) record
controlled evidence for the expanded cases. The subsequent
[90% branch coverage report](experiments/2026-10-28-branch-90.md) records the
latest measurement and the additional boundary and repair tests.
