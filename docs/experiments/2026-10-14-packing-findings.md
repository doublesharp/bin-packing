# Packing experiment findings, 2026-10-14

Seven experiments were implemented and reviewed, with retained changes prepared for v0.5.0. These are targeted fixture results, not representative production benchmarks or claims of research novelty.

| Experiment | Decision | Measured result |
| --- | --- | --- |
| Physical pattern recombination | Keep as opt-in Rust API | Auto 6 sheets to 5, waste 770 to 370. Another fixture improves offcut sum-of-squares 49,928 to 51,950 at the same 8-sheet count. |
| Joint 3D repair | Keep as opt-in Rust API | LocalSearch 3 bins to 2 for widths 6,5,3,2,2,2 in length-10 bins. |
| Demand-aware placement and mixed rotations | Keep as opt-in experiments | Specified restricted heuristics improve 2 sheets to 1. Full Auto already uses 1; no count gain over full Auto in 500 diagnostic iterations, some repeated. |
| Conservative-scale 3D bounds | Keep in supported exact backend | Seventeen side-40 cubes: lower bound 2 to 3, expanded nodes 24,310 to 17, median 104.801 ms to 0.071 ms. Easy cases pay fixed bound overhead. |
| Canonical saw states | Keep for sequential search | Repeated-sheet median 0.871 ms to 0.114 ms, generated states 4,992 to 178, same effort 23. Single-sheet median rises about 4 microseconds. |
| Forecast offcut valuation | Keep as opt-in Rust API | Feasible future fulfillment 0 to 1 with unchanged current sheet count and cost. Depends on the supplied forecast and physical extraction. |
| Compressed multi-stock 1D search | Keep as opt-in Rust API | Auto 5 bars to 4, waste 11 to 2, cost 6 to 4, with complete-search exactness on the small fixture. Adds search time. |

Batched saw canonical pruning was rejected after an initial roughly eightfold slowdown with no quality gain. Batching now bypasses the cache; final median 1.792 ms versus original 1.814 ms, with the same complete output apart from new zero/false diagnostics.

## Scope and verification

Ordinary output hashes and full outputs matched on ten unaffected benchmark cases. All 16 ordinary input hashes matched. Measurements used release builds, fixed seeds, one Rayon thread, an untimed warmup and serial samples. Timings include input cloning and exclude compilation/serialization. The host was macOS 15.7.7 arm64 with 12 logical CPUs and Rust 1.99.0. Some fixtures were discovered through seeded exploration. Timing variation on unchanged solvers prevents general speed claims.

Geometry, rotation permissions, kerf, stock caps and demand accounting have independent regression coverage. Conservative bounds were checked against 1,140 tiny shape triples. Arc selection was checked against independent per-piece assignments for 96 tiny integer/fractional-cost cases. Pattern selection has exhaustive count-allocation regressions. Restricted pools and bounded heuristics do not prove unrestricted optimality.

At version preparation, local checks passed 413 Rust tests, seven Python checks, two doctests, eight Rust and nine WASM feature combinations, fuzz-target compilation/clippy, crate packaging, 43 Node/WASM tests, 13 declaration checks, 300 solver-finder cases with 1,800 calls and four Chromium package variants. Independent source reviews passed. These counts record that run; release CI validates the tagged revision separately.

## Reproduction and evidence

The benchmark example is `crates/bin-packing/examples/packing_experiment_bench.rs`. Run `scripts/run-packing-experiments.py` with separate baseline/candidate checkouts, configured Scratch builds, and an output path under `/Volumes/Scratch/bin-packing/reports/`. The baseline is pinned to `0b626ac920d2ef5845c5e1763a2999ee5baf0c29`; the final algorithm measurement used `1c05c74318f610b35418a9c79e10b4c7819831fa`.

Raw initial/final measurements, verification receipts and review records were moved out of git, hash-verified and preserved under `/Volumes/Scratch/bin-packing/reports/release-v0.5.0-20261017/docs/experiments/`. Test-input JSON fixtures remain in the crate because they are reproducible source inputs, not run receipts.

All implementation history is preserved through the common branch. Eleven temporary worktrees were archived after integration, reclaiming 14.49 GiB of owned builds. See [v0.5.0 release notes](../releases/v0.5.0.md) for public entrypoints, compatibility changes and limitations.
