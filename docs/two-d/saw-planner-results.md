# Saw planner verification and measurements

This is the initial implementation snapshot. The subsequent [test audit](saw-planner-test-audit.md) documents stronger validation, corrected test assumptions, lazy baseline construction and updated measurements. Raw files below preserve the original run.

Measured September 9, 2026 on Apple M2 Max, macOS arm64, Node v24.16.0, Chromium 151.0.7922.34. Rust and bindings use release builds for measurements. Each workload has one warmup followed by seven measured calls; the table reports median and observed maximum, not a statistical tail estimate. Timings include object serialization, planner validation/search and returned-object decoding. Module initialization and packing are excluded. The browser measurements run the actual web WASM modules in headless Chromium over a loopback HTTP server.

## Repeated strips

For four 300×600 parts on a 1200×600 zero-kerf sheet, legacy `plan2dCuts` emits cuts at X=300,600,900 and reports two fence resets. The new job uses:

| Pass | Current workpiece X range | Original cut X | Reference edge | Rip setting |
| --- | --- | --- | --- | --- |
| 1 | 0–1200 | 300 | Left | 300 |
| 2 | 300–1200 | 600 | Left | 300 |
| 3 | 600–1200 | 900 | Left | 300 |

The physical result is three passes, one initial setup, zero setting changes and three loads. With kerf 3 and stock 1209×600, blade bands are [300,303], [603,606] and [906,909]. The same 300 setting produces all four required parts in three passes. No gap-cleanup pass is emitted.

## Weighted and primary objectives

The asymmetric weight fixture uses a 23×19 sheet and rectangles `(x,y,width,height)` at `(0,0,5,9)`, `(5,0,8,8)`, `(13,0,2,7)`, `(13,7,2,2)`, with zero kerf and panel-saw settings. Search uses 30,000 successors and beam width 64. These are geometric test units; uniform scaling preserves this fixture's relationships.

| Objective/weights | Cuts | Initial setups | Changes | Rotations | Loads | Effort |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rotation=100, change=0 | 7 | 1 | 4 | 3 | 7 | 319 |
| Rotation=0, change=100 | 6 | 1 | 2 | 4 | 6 | 217 |

Other weights remain cut=1, initial=5, handling=1. Scores across these two rows use different models and are not directly comparable. The current weight regression uses a smaller fixture whose optimum is also verified by exhaustive search; these rows preserve the original schedule experiment.

A separate fixture uses `(0,0,10,7)`, `(10,0,7,8)`, `(10,8,7,5)`, `(10,13,4,6)` on the same stock. With default effort weights, cut-first returns six cuts and four changes, effort 43. Setup-first returns seven cuts and three changes, effort 44. This is an actual decomposition/operation tradeoff, not rescoring a fixed list.

## Cross-sheet batching

Duplicate the first asymmetric fixture onto two sheets. Sequential planning returns 15 passes, one initial setup, five changes, 13 rotations and 15 loads, effort 86. Batching returns 14 passes, one initial setup, three changes, eight rotations and 14 loads, effort 64 under the same model and limits.

The batched passes reuse panel-stop settings in this actual sheet order:

| Shared setting | Sheets of successive passes |
| --- | --- |
| 5 | 0, 1 |
| 8 | 0, 1, 0, 0, 1, 1 |
| 9 | 0, 0, 1, 1 |
| 7 | 0, 1 |

The full operation lists, including loads, rotations and setup instructions, are in [schedule evidence](saw-planner-data/schedules.json). Each pass has its current bounds, diagram segment and reference edge. This is sequential batching, not stacked material. Do not execute a per-sheet filtered view independently.

## Bounded performance and larger-job quality

The grid workload uses 145×97 panels with kerf 3. The 32-part case is 8×4; 64 parts is 8×8 on 1181×797 stock; 128 parts uses two such sheets with batching. All use default weights, 4,000 optimization evaluations and beam width 32.

| Parts | Native Node median/max ms | Node WASM combined median/max ms | Browser WASM combined median/max ms | Browser WASM two-d median/max ms |
| --- | --- | --- | --- | --- |
| 4 | 0.074 / 0.094 | 0.109 / 0.141 | 0.1 / 0.2 | 0.1 / 0.2 |
| 32 | 3.449 / 3.597 | 2.428 / 2.598 | 2.3 / 2.6 | 2.2 / 2.5 |
| 64 | 4.164 / 4.344 | 2.592 / 2.824 | 2.5 / 2.6 | 2.4 / 2.6 |
| 128 | 4.999 / 5.182 | 3.077 / 3.232 | 3.0 / 3.1 | 2.8 / 3.0 |

The 32- and 64-part schedules reduce baseline setup changes from 12 to 2. The 128-part job reduces changes from 26 to 2. Effort decreases from 154 to 104, 218 to 168, and 436 to 316 respectively. Physical passes remain 31, 63 and 126. These larger searches consume the entire optimization budget and report heuristic results. The completed baseline-tree scheduling pass obtains these improvements before broader beam search runs out of effort.

These results measure bounded responsiveness on regular grids, not worst-case construction or arbitrary production workloads. Highly fragmented layouts can exhaust the separate baseline budget and fail explicitly. Limits cap inputs, geometry cache and state evaluations; they do not impose a wall-clock deadline. The API does not model carrying distance, machine capacity, blade changes or arbitrary auxiliary waste cuts.

Raw measurements include effective limits, explored states, baseline counts and returned counts: [native and Node WASM](saw-planner-data/node-performance.json), [Chromium WASM](saw-planner-data/browser-performance.json).

## Reproduction and coverage

After building both binding packages:

```sh
node --test bindings/node/test/smoke.test.js
node --test bindings/wasm/test/smoke.test.mjs
node bindings/test/saw-schedules.cjs
node bindings/test/saw-performance-node.cjs
node bindings/test/saw-performance-browser.cjs /path/to/node_modules/playwright
node bindings/test/saw-types.mjs
```

The last command prints a temporary TypeScript file containing contextual assignments from actual native/combined/two-d runtime values. Compile it with `tsc --strict --noEmit --lib es2022,dom --module esnext --moduleResolution bundler <printed-path>`. The check uses packaged WASM declarations, verifies shared declaration blocks remain identical, and exercises nullable ancestry fields and all operation kinds. TypeScript 6.0 was used for this run.

`cargo test -p bin-packing --test saw_plan_integration --locked` exercises independent replay, initial/setup accounting, coordinate distinctions, duplicate labels, multiple stock sizes, grain locks, nonzero kerf, clipped trims, factory edges, waste, non-slicing layouts, overflow inputs, bounded backtracking, deterministic exhaustion, independently checked objective bounds, batching and larger-job quality. It includes 48 generated physical slicing layouts and an independent tiny oracle that scans integer blade positions instead of using planner candidates, including kerf, partial edge relief, waste, both machine types and all three objectives.

The full repository gate is `./scripts/check-quality.sh`: formatting, TOML/text hygiene, all-target/all-feature Clippy with warnings denied, workspace tests and private-item documentation. Existing cutting integration tests and native/WASM smoke tests continue to cover legacy behavior. No package publication or Subchecker changes are part of this work.
