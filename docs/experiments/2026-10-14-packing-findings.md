# Packing experiment findings, 2026-10-14

All seven ideas were implemented in separate managed worktrees and merged into `feature/packing-experiments`. The original `main` checkout remains at `0b626ac920d2ef5845c5e1763a2999ee5baf0c29`. The common branch is for testing. No release or remote push was made.

The strongest results are pattern recombination, joint 3D repair and the stronger exact 3D bound. The compressed 1D search improves a mixed-stock fixture but adds integer search work. Forecast reuse produces a useful layout when a forecast is available. Demand-aware placement and mixed rotation counts remain experimental, since the full ordinary Auto portfolio already matches their committed layouts.

Evidence is limited to targeted fixtures. Some were found through seeded exploration rather than blind holdouts. The algorithms combine known packing techniques; this work does not establish research novelty.

## Decisions and quality results

| Experiment | Decision | Observed result | Main limit |
| --- | --- | --- | --- |
| Cross-solver physical pattern recombination | Keep, explicit API | Auto 6 sheets to 5 on repeated demand, waste 770 to 370. Another fixture keeps 8 sheets and improves offcut sum-of-squares 49,928 to 51,950. | Only whole harvested patterns are searched. No unrestricted optimum certificate. |
| Joint target-bin repair | Keep, explicit API | LocalSearch 3 bins to 2 for widths 6,5,3,2,2,2 in length-10 bins. | At most 48 items and 4 bins per neighborhood, seeded greedy placement. |
| Demand-aware placement and mixed rotations | Keep as opt-in experiments | Specified MaxRects and uniform RotationSearch fixtures improve 2 sheets to 1. Full Auto already uses 1. | No sheet-count gain over full Auto in 500 diagnostic iterations, including repeated inputs. |
| Integer conservative-scale 3D bounds | Keep in existing supported exact backend | Seventeen side-40 cubes in a side-100 bin need 3 bins. Bound 2 to 3, search 24,310 nodes to 17. | Existing single uncapped stock, XYZ-only exact domain. Fixed bound cost can slow easy mixed jobs. |
| Canonical physical saw states | Keep for sequential saw search | Repeated-sheet sequential fixture generates 4,992 states before versus 178 after, with the same effort and replayable plan. | The initial batched implementation was about 8 times slower with no quality gain, so batching bypasses this cache. Beam search remains heuristic. |
| Forecast offcut valuation | Keep, explicit API | Same current sheet and cost, feasible forecast fulfillment 0 to 1. An 8x10 remnant beats a larger 29x4 strip for an 8x8 future part. | Depends on caller forecast and greedy physical extraction. |
| Compressed multi-stock 1D integer search | Keep, explicit API | Auto 5 bars to 4, waste 11 to 2 and cost 6 to 4, with complete-search exactness on the small fixture. | Bounded exponential selection search. Graph compression alone does not make it fast or exact. |

## Public entrypoints

All added searches are Rust-only. Existing Node and Wasm bindings retain their solver entrypoints. The stronger 3D bound and sequential saw-state pruning apply through the existing dispatch paths. Batched saw jobs retain their original beam behavior. Saw search reports gain additive, serde-defaulted duplicate/cache counters.

- `two_d::solve_2d_recombined(problem, options)` pools complete physical sheets from the Auto portfolio. It preserves source demand indices even when names repeat, hard stock caps and the best original candidate. Recombination is capped at 16 demand/stock types, 96 parts, 256 patterns and 25,000 selection states. Ordinary Auto fallback is limited to 128 parts. Beam width and multistart runs must be at most 32. Fractional or very large costs disable memo dominance so the existing floating-point comparison remains valid.
- `three_d::solve_3d_repaired(problem, options)` retains the requested ordinary solver as incumbent. It jointly reconstructs target and destination bins and permits neutral exploratory rearrangements while retaining the true best result. Up to 1,024 neighborhoods are added. It preserves existing unplaced items and skips guillotine-required jobs. Its added placement work is bounded, while incumbent construction and whole-solution comparisons depend on input size.
- `two_d::solve_2d_demand_aware(problem, options)` scores up to 16 geometric candidates by lost remaining-demand cavities. It uses the best grid capacity within one free rectangle, avoiding sums over overlapping maximal rectangles.
- `two_d::solve_2d_mixed_rotations(problem, options)` searches per-demand rotated counts and bounded seeded item orders. It performs at most 128 trials. Both explicit 2D experiments preserve the requested baseline, accept at most 128 parts, 64 demand types and 8 stocks, reject duplicate stock names under ordinary validation, and skip guillotine-required jobs. Count-vector coverage does not prove placement completeness.
- `two_d::reuse::solve_2d_with_reuse(problem, options, forecast, reuse_options)` ranks current layouts by feasible future fulfillment in disjoint terminal saw remnants. `evaluate_reuse(job, forecast, reuse_options)` evaluates a supplied physical job. Default material ceilings preserve baseline sheet count and cost; an explicit override can relax those ceilings. Hard stock inventory always applies. Results include source-index maps. At most 8 top-level candidate solves and 512 future placements are allowed. Mixed blade kerfs need separate saw jobs, and physically nondecomposable layouts return typed errors.
- `one_d::solve_1d_arc_flow(problem, options)` builds per-stock layered count DAGs and streams feasible patterns into integer selection. It preserves the Auto incumbent and procurement estimate. Graph nodes plus arcs and memo storage use `exact_pattern_limit`; traversal work uses that limit times `max(column_generation_rounds, 1)`. It accepts at most 128 stock/demand rows and 4,096 cuts, with selected-bar depth at most 128. Complete search alone reports `exact=true`; caps retain the best result with `exact=false`. Incumbent/procurement work lies outside the graph budget.

The explicit 2D APIs also reject aggregate areas that could overflow the existing `u64` solution accounting, before invoking ordinary seeds. All public docs state their work and input bounds. These budgets limit work; they are not wall-clock deadlines.

## Measurement method

The [final measurements](data/2026-10-14-packing-measurements.json) compare original commit `0b626ac920d2ef5845c5e1763a2999ee5baf0c29` with combined code commit `1c05c74318f610b35418a9c79e10b4c7819831fa`. The [initial measurements](data/2026-10-14-packing-initial.json) preserve the rejected batched-cache result at `7558dcb`.

Both checkouts use release builds, the same fixture/options/seed JSON, one Rayon thread, one untimed warmup, and serial samples. There are 21 samples per case, 31 for several small cases, and 11 for repeated-sheet saw cases. Timings include input cloning and exclude compilation and JSON serialization. Host: macOS 15.7.7 arm64, 12 logical CPUs, Rust 1.99.0. The data stores complete inputs and outputs, sample counts, medians, p95 values and canonical input SHA-256 hashes. Peak process RSS was not measured.

Default-path measurements, all times in microseconds:

| Case | Original median | Final median | Final p95 |
| --- | ---: | ---: | ---: |
| bounds-nine | 17.958 | 39.166 | 41.834 |
| bounds-seventeen | 104,800.833 | 70.959 | 74.541 |
| bounds-mixed | 31.709 | 47.459 | 51.500 |
| saw-sequential | 870.542 | 113.708 | 128.625 |
| saw-batched | 1,813.500 | 1,791.708 | 2,589.083 |
| saw-single | 26.666 | 30.709 | 33.542 |

The seventeen-cube fixture keeps the same three bins, raises the valid lower bound from two to three, and reduces expanded nodes from 24,310 to 17. The nine-cube and mixed fixtures keep their bin counts but gain valid count-optimality certificates at a fixed time cost. Sequential repeated-sheet planning keeps effort 23 and reduces generated successors from 4,992 to 178, with 77 duplicate rejections and a peak of 555 dominance units. It now finishes without beam pruning. The single-sheet control reduces successors 84 to 58 but adds about four microseconds.

The final batched plan matches the entire original decoded output after excluding the three new zero/false diagnostics. Both generate 14,678 successors and have effort 23. Final median is 1.792 ms versus 1.814 ms for the original, within the observed timing variation. This confirms the expensive batched cache has been removed.

Explicit searches include their ordinary seed solves. The ordinary column below uses the same common binary and requested options:

| Case and API variant | Ordinary median | Opt-in median | Opt-in p95 |
| --- | ---: | ---: | ---: |
| pattern-count, recombined | 2,821.709 | 2,641.292 | 5,285.583 |
| pattern-offcuts, recombined | 3,127.875 | 2,744.042 | 4,142.250 |
| demand-space, demand-aware | 6.167 | 7.208 | 8.125 |
| mixed-rotations, mixed-count | 42.125 | 203.250 | 596.833 |
| repair-strip, joint-repair | 74.042 | 139.708 | 538.417 |
| arc-mixed, arc-flow | 2,684.042 | 6,096.333 | 30,656.917 |
| arc-repeated, arc-flow | 475.958 | 2,096.292 | 9,636.625 |
| forecast-reuse, forecast | 9.750 | 433.708 | 1,061.667 |

Full Auto already uses one sheet for both demand-space and mixed-rotation examples. Its measured medians are 214.375 and 1,527.625 microseconds respectively. The explicit alternatives match that count with shorter measured runtimes on these small fixtures. A diagnostic family of 500 iterations found no count improvement over full Auto, and some inputs repeated. This evidence supports targeted opt-in use rather than a default portfolio change.

For repeated 1D demand, the graph represents 7,307,872,109 patterns with 317 nodes and 3,486 arcs. Retained graph allocations total 97,536 bytes, excluding the build map, memo, allocator overhead and other solver state. The 10,000-work limit stops selection with the original 16-bar incumbent and `exact=false`; compression does not produce a quality improvement on that fixture. The mixed-stock fixture completes in 35,018 work units with four bars and `exact=true`.

All 16 ordinary input hashes match. Complete decoded outputs match exactly on all ten cases outside the deliberately changed bound/saw paths. New layouts independently pass the committed physical/accounting regressions. Timing differences alone do not establish a general speed benefit.

The initial measurement at `7558dcb` is preserved separately. Sequential repeated-sheet saw planning fell from 902.625 to 113.833 microseconds median, but batching rose from 1,687.917 to 13,540.750 microseconds with identical effort 23. Batched generated successors fell only from 14,678 to 13,636 while variable-size future keys, Pareto bookkeeping and retained-successor filtering added work. The table reached 65,533 scalar units and saturated. This optimization was therefore rejected for batching, and the final branch bypasses all future-key/table work for batched jobs. Sequential pruning remains.

The single-sheet control initially changed from 26.708 to 29.792 microseconds, while successors fell from 84 to 58. The bound also has a fixed cost: the initial nine-cube and easy mixed fixtures became slower while gaining a valid count-optimality certificate. These tradeoffs guided the retained scope.

Unchanged ordinary solvers varied substantially between the baseline and candidate passes. For example, mixed-stock 1D Auto had identical complete output but measured 4.363 versus 1.649 milliseconds in the initial pass. Identical outputs and timing variation prevent attributing that difference to an algorithm change. Exact quality/state changes carry more weight than small timing differences on this host.

## Correctness and review

Tests independently check demand accounting, allowed orientations, stock availability, bounds, kerf gaps and physical saw replay. The 3D bound is checked against all 1,140 tiny shape triples with an independent unit-cell packing oracle, plus small 1D partitions. Arc selection is checked against independent per-piece assignment for 96 tiny integer/fractional-cost cases. Pattern selection has an exhaustive count-allocation oracle, including order-sensitive fractional and large costs.

Two rounds of focused independent review found and corrected unsafe floating-cost memo dominance, original-name validation bypass, restored-cost ordering, typed error loss, aggregate-area overflow and unnecessary copying. Fresh corrective review passed every task. The final combined-branch source review passed. A later root check found missing TypeScript declarations for the new saw counters. Commit `7558dcb` adds all three fields to native/full-Wasm/2D-Wasm declarations, plus compile-time field access and runtime smoke checks. A TS2339 red was observed before the fix; standalone compilation and independent follow-up source review passed after it.

The final combined code commit is `1c05c74318f610b35418a9c79e10b4c7819831fa`. Every command below exited zero after the batched-cache correction. The [verification receipt](data/2026-10-14-packing-verification.json) records exact commands, counts, output comparisons and log hashes. These are local checks; CI and deployment were not run.

- `CARGO_BUILD_JOBS=2 ./scripts/check-quality.sh`: 409 Rust tests passed, one manual benchmark ignored, four Python tests passed, benchmark smoke cases passed, rustfmt, taplo, typos, workspace all-target/all-feature clippy with `-D warnings`, and private rustdoc passed.
- `cargo test -p bin-packing --doc --all-features --locked` through the scratch wrapper: two doctests passed.
- `CARGO_BUILD_JOBS=2 ./scripts/check-features.sh`: eight Rust feature combinations, nine Wasm combinations, and fuzz-target formatting/clippy compilation passed. No fuzz campaign was executed.
- `./scripts/check-bindings.sh` with isolated TypeScript 5.9.3 and installed Playwright: 12 native Node and 31 Wasm tests passed, packed package contracts passed, 13 declaration entrypoints type-checked, 300 solver-finder cases made 1,800 calls, and four package variants passed in Chromium 151.0.7922.34.
- The paired release runner completed and verified all ordinary input hashes. The final batched output matches the original after excluding the new zero-valued diagnostics.

Independent focused reviews, corrective reviews, and [combined source review](2026-10-14-packing-review.md) passed. The review record includes follow-ups for the consumer declarations and sequential-only pruning restriction.

Cleanup is complete. Eleven temporary managed worktrees were archived with recoverable snapshots. Only the common testing worktree remains attached. Owner-checked disposable builds freed 14.49 GiB, and the temporary TypeScript compiler was removed. The common build cache and lightweight diagnostic logs are retained for testing and evidence. Original main is clean at the baseline commit. No branches were force-updated, and nothing was pushed or published.

## Reproduction

The JSON measurements retain complete inputs, options, outputs, medians, p95 values and canonical input SHA-256 hashes. `crates/bin-packing/examples/packing_experiment_bench.rs` contains the cases. `scripts/run-packing-experiments.py` builds baseline and candidate serially and rejects mismatched ordinary inputs.

Before cleanup, reproduction used a separate baseline worktree pinned to the commit above:

```sh
python3 scripts/run-packing-experiments.py \
  --baseline /path/to/pinned-baseline \
  --candidate /path/to/feature/packing-experiments \
  --output /tmp/packing-measurements.json
```

The runner copies only its measurement example and fixture data into the baseline and removes the block using new APIs. It never changes baseline solver source. Both checkouts must have scratch builds configured as documented in `docs/scratch-builds.md`.

## Implementation history

| Task | Original implementation | Review corrections |
| --- | --- | --- |
| 1 | `7e27d25` | `ac561da` |
| 2 | `89d0204` | `09d4136` |
| 3 | `6d44875` | `dcdc1f3` |
| 4 | `1c3ee63` | No corrective implementation needed |
| 5 | `2165314` | Complexity documentation in `09d4136`, consumer types in `7558dcb`, batched cache bypass `207135e` after measurement |
| 6 | `31baecf` | `a3469dc`, `6446263` |
| 7 | `80dfcf0` | `d3000fa` |

All implementation and corrective commits are preserved in the common branch's merge history. Benchmark runner commit `a918a44` adds reproducible fixtures and Auto controls. Consumer-declaration correction `7558dcb` completes the saw report diagnostics. The final source review is retained beside this report.

The final merge commit `1c05c74` includes the saw restriction. Later commits only save documentation and measurement evidence.
