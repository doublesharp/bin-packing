# Workspace verification and efficiency follow-up

September 9, 2026. This continues the [saw-planner test audit](two-d/saw-planner-test-audit.md) across the Rust workspace, native Node package, twelve WASM entry points and separate fuzz project.

## Fixed defects

- Rotation search shifted a single u64 mask by 64 or more bits. Sampled assignments now use as many words as needed. Regressions cover 63, 64, 65 and 129 rotatable demand types.
- Pre-swapping dimensions erased rotation provenance and changed unplaced demands. An internal fixed-orientation field now preserves source dimensions, rotation permission and placement flags. A rotated 1×2 part on 2×1 stock also passes directly into saw planning. The 3D layer, column and wall adapters retain their original unrestricted behavior.
- Raising the exhaustive rotation threshold to 63 caused a capacity-overflow panic. Exhaustive search now has a hard ceiling of 16 types, preserving the default threshold. Larger jobs use `multistart_runs`. At most 32 completed candidates are retained at once instead of every assignment's layouts. Assignment construction scans demand indices once rather than searching them repeatedly.
- Large 2D solver results could not feed `plan2dCuts` in either binding. JavaScript serializes large u128 metrics in exponent notation, which Rust's integer decoder rejects. Both cut bindings now decode only the geometry that legacy cut planning consumes. Rust solution serialization and public JavaScript result shapes remain unchanged.
- Web initialization declarations incorrectly described raw WASM exports as ergonomic object functions and omitted `initSync`. They now reference the declarations generated for the exact WASM binary. Type tests reject treating a raw ABI tuple as a solver result.
- The release workflow downloaded native binaries but omitted the generated `index.js` loader. It now transports that loader and tests the prepared tarball before publishing. All native release matrix jobs also run smoke tests after their builds.
- Dimension-specific Rust tests and benches could not compile without all features; WASM's JSON-only feature could not compile; native feature flags unintentionally inherited every core default feature. Target requirements, conditional helpers and dependency flags now match the intended builds.
- Individual WASM `build:*` commands bypassed package normalization. Each now uses the common build script, including CJS renaming and declaration installation, and rebuilds only the requested flavor.

## Performance evidence

Beam search now ranks lightweight moves before allocating operation traces and successor states. Selected moves retain only their selected split, allowing unused geometry to be released. Completed candidates still update the incumbent immediately, and comparison, budget and pruning order are preserved. Thirty-six fixed job/option combinations matched the previous native output exactly.

The large resource probe uses 512 strips, beam width 256 and 100,000 optimization states. Both versions return 511 cuts and consume exactly that budget. The revised beam uses 1.71 billion retired instructions versus 2.79 billion for the previous eager beam, about 39% fewer. Peak physical memory footprint remains approximately 13.4 MB for both versions. User CPU time was 0.10 versus 0.14 seconds. Concurrent compilation made wall-clock samples noisy, so these are local resource measurements, not latency guarantees.

The grid benchmark uses the existing 145×97 panels with kerf 3, default weights, 4,000 optimization states, one warmup and seven measured calls. It independently replays returned geometry after timing. Raw before/after measurements are in [beam-before.json](two-d/saw-planner-data/beam-before.json) and [beam-after.json](two-d/saw-planner-data/beam-after.json). Compare medians and maxima; returned pass counts, setup changes and effort remain unchanged.

| Parts | Native median before / after ms | Native observed maximum before / after ms |
| --- | --- | --- |
| 4 | 0.084 / 0.072 | 0.129 / 0.104 |
| 32 | 3.005 / 0.939 | 3.243 / 1.231 |
| 64 | 3.373 / 1.526 | 3.516 / 1.812 |
| 128 | 3.031 / 2.039 | 3.130 / 2.261 |

These samples show 33–69% lower median latency on the 32–128-part fixtures. Final native and Node WASM grid/strip measurements are also retained in [beam-after-bindings.json](two-d/saw-planner-data/beam-after-bindings.json).

Reproduce the resource workload on macOS:

```sh
cargo build -p bin-packing --example saw_stress --release --no-default-features --features two-d --locked
/usr/bin/time -l target/release/examples/saw_stress
```

The resource probe's cut counts and search budget are portable. The profiler command and instruction/footprint counters are platform-specific.

Raw resource counters are retained in [beam-resources.json](two-d/saw-planner-data/beam-resources.json).

## Repeatable tools

From the repository root:

```sh
./scripts/check-quality.sh
./scripts/check-features.sh
TSC_PATH=/path/to/typescript/lib/tsc.js \
PLAYWRIGHT_PATH=/path/to/node_modules/playwright \
./scripts/check-bindings.sh
```

The feature matrix exercises eight core/native combinations and nine WASM combinations, then formats and lints both separate fuzz targets. Mixed-dimension integration suites declare required features; dimension unit tests and relevant integration suites still run independently.

The binding tool installs the locked native build dependencies, builds both packages, runs smoke and contract tests, packs and loads both npm archives in temporary directories, compiles runtime values against all thirteen declaration entry points, runs the seeded finder and checks all four web variants in Chromium. Use `--skip-build` only after building the current tree. Without tool-path variables it resolves `tsc` and `playwright` normally. Archives use a temporary npm cache and are never published by the tool.

The counterexample finder is dependency-free after binding builds:

```sh
node bindings/test/solver-finder.cjs --seed 42 --cases 1000
node bindings/test/solver-finder.cjs --replay fuzz/artifacts/solver-finder.json
node --test bindings/test/finder.test.cjs
```

It generates bounded asymmetric 1D, 2D and 3D problems with varying inventory, kerf and rotation permissions. Each input runs twice through native, combined WASM and the corresponding specialized WASM binding. It checks dimensions, demand accounting, inventory, non-overlap, adjacent-edge kerf gaps, costs, area/volume metrics and reproducibility. Deterministic constructors must also agree across backends. Sampled algorithms may differ between 32-bit WASM and 64-bit native because `SmallRng` uses different implementations; each result must still pass physical checks and reproduce within its backend. Failures save the exact input, options, seed and case index for replay. The first failure stops the run. Mutation tests prove it rejects missing demand, incorrect rotation metadata and insufficient adjacent kerf while accepting valid diagonal separation.

## Verification and limits

Local verification uses macOS arm64, Node v24.16.0 and Chromium 151.0.7922.34. Native package checks cover the available host binary; the other five platform binaries require the configured release CI runners. All twelve WASM entry points are loaded through package exports, both before and after packing. Browser checks execute the four web variants, and bundler entry points additionally execute under Node's experimental WASM module loader. This does not certify every third-party bundler's tree-shaking configuration.

The full quality gate passed 344 Rust tests. Binding suites passed 43 tests, in addition to archive, declaration and browser checks.

The finder passed 2,300 jobs across three seeds, totaling 13,800 solver calls including repeats. Bounded sanitizer runs completed 4,000 inputs for `solver_inputs` and 2,000 for `three_d_modules`, using existing corpora. The first run exposed an invalid old fuzz assertion: `guillotine_required` filters Auto's candidate set and does not override an explicitly selected algorithm. The assertion now applies only to Auto, matching the documented API. The fuzz project also passes its own formatting and Clippy checks.

Fuzz runs are reproducible with the existing nightly toolchain and cargo-fuzz:

```sh
cargo +nightly fuzz run solver_inputs -- -runs=4000 -max_len=2048
cargo +nightly fuzz run three_d_modules -- -runs=2000 -max_len=2048
```

These tests are bounded evidence, not exhaustive correctness proofs. No package was published and no other repository was modified.
