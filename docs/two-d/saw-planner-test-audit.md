# Saw planner test audit and performance repair

September 9, 2026. This audit covers the new workpiece-aware planner and its Rust, native Node and WASM tests. The [initial measurements](saw-planner-results.md) remain available as a historical snapshot.

## Test findings

| Weak test or assertion | Evidence and correction |
| --- | --- |
| Replay checked child area totals without exact geometry | An overlapping partition with the correct summed area passed. Replay now derives the exact child rectangles and blade band independently. |
| Incomplete machine, kerf and diagram checks | Mutations changing a table-saw accessory to a panel stop, blade width from 3 to 100, or the cut segment to X=0 passed. Replay now checks the requested machine accessory, source kerf, reference-side material, orientation and full-span segment. |
| Missing provenance and classification checks | Removed ancestry/views, duplicate finished-part IDs and altered offcut flags could escape checks. Replay now reconstructs parents, creation dependencies, part identities, offcut classification and complete sheet views. |
| Edge-relief fixture did not prove relief was necessary | Replay now rejects a partial blade band when relief is disabled. |
| Equal weights hid coefficient mixups | Substituting the cut coefficient for handling passed all 19 original tests. Distinct coefficients now exercise every operation count; the same mutation fails with reported effort 139 versus expected 237. Production scoring was correct; this was a test hole. |
| Grain-lock rejection had two reasons to fail | The old fixture also mismatched rotated dimensions. The replacement has valid rotated dimensions and changes only rotation permission between rejection and acceptance. |
| Heuristic comparisons required another plan to remain worse | Replace accidental strict inequalities with fixed feasible quality bounds. Verify the weight-sensitive schedule on a tiny asymmetric fixture against exhaustive optima and cross-score both results. |
| Replay ignored the no-batching execution contract | Relabeling an interleaved schedule as non-batched passed Rust and JavaScript replay. Both now require nondecreasing sheet indexes when batching is disabled; isolated mutations verify rejection. Production ordering was correct. |
| Performance fixtures omitted long strip jobs | Add 128-, 256- and 512-part strips with kerf, full output replay and bounded baseline attempts. This exposed a production construction-budget failure. |

The independent Rust oracle scans integer blade positions rather than calling planner geometry helpers. It now covers nonzero kerf, waste, partial edge relief, both machine types and all three objectives. Its exhaustive fixtures are deliberately tiny and single-sheet; it does not establish global optimality for larger jobs. The larger fixtures use fixed quality bounds and complete physical replay. Generated slicing fixtures cover 48 additional layouts.

Native and WASM smoke tests share a browser-compatible consumer replay. Eight isolated corruption cases check ancestry, dependencies, segments, finished-part identities, blade width, accessories, views and sheet ordering. Runtime values are also checked against the packaged TypeScript declarations.

## Production repair

Baseline construction previously materialized and scored every possible split and both reference edges before attempting one. A simple 128-part strip job consumed 16,256 baseline evaluations; 256 and 512 parts failed with the default 20,000 limit.

Construction now enumerates cheap boundary descriptors, scores machine transitions without successor allocation, and validates one geometric split at a time. It uses the cheaper reference edge for that split because reference orientation affects effort but cannot change geometric feasibility. Iterative backtracking still handles choices that strand untrimmable margins. The 512-part regression completes in 511 attempted splits under a 512-attempt budget, with 511 passes, one initial setting and no setting changes.

The baseline-tree scheduling pass now reconstructs only its known cut per workpiece, scores alternatives without building full states, and builds the selected successor once. Both that pass and broader search check exhausted budgets before expanding geometry. State application also avoids cloning and then rebuilding the ready-workpiece vector.

`baseline_states` now counts attempted geometric splits, including candidates rejected for intersecting parts. Boundary enumeration and reference-edge scoring are outside that count. Defaults and maximum limits are unchanged. For p contained parts, baseline candidate enumeration/scoring costs O(p log p), and each attempted partition costs O(p). This avoids eagerly constructing O(p) partitions at each baseline step; it does not make an entire strip job linear. Broader beam geometry still costs O(p²) per uncached workpiece query.

## Measurements

Apple M2 Max, macOS arm64, Node v24.16.0 and Chromium 151.0.7922.34. Release builds, one warmup and seven measured calls per workload. Values include serialization, validation, search and decoding; replay runs after timing. These are short local samples, not latency guarantees. Grid inputs and optimization budgets are identical before and after.

| Grid parts | Native median before → after ms | Node combined WASM median before → after ms | Node two-d WASM median before → after ms |
| --- | --- | --- | --- |
| 4 | 0.066 → 0.072 | 0.107 → 0.111 | 0.106 → 0.102 |
| 32 | 3.467 → 3.175 | 2.338 → 2.192 | 2.393 → 2.327 |
| 64 | 4.134 → 3.542 | 2.596 → 2.383 | 2.653 → 2.338 |
| 128 | 4.977 → 4.406 | 3.108 → 2.327 | 3.125 → 2.295 |

The 32/64/128-part outputs retain 31/63/126 passes, two setup changes and effort 104/168/316. All consume their 4,000-state optimization budgets and report heuristic results. Native median time improves about 8–14% on these larger grids. The observed native maximum increases from 3.652/4.305/5.162 ms to 5.721/7.091/7.427 ms, so the samples do not establish better worst-case latency. Raw data retains both median and maximum.

The strip workload uses 145×97 panels, kerf 3 and `max_states: 0`, which isolates baseline construction plus validation and serialization. Browser results use actual web WASM modules in headless Chromium.

| Strip parts | Baseline attempts / passes | Native median ms | Node combined WASM median ms | Node two-d WASM median ms | Browser combined WASM median ms | Browser two-d WASM median ms |
| --- | --- | --- | --- | --- | --- | --- |
| 128 | 127 | 1.557 | 2.075 | 1.624 | 1.5 | 1.4 |
| 256 | 255 | 4.336 | 7.636 | 4.546 | 4.1 | 3.9 |
| 512 | 511 | 15.934 | 29.510 | 14.762 | 12.7 | 12.8 |

Every strip plan has one initial setup and zero changes. The original failure probe used 300×600 strips; the new timing series uses the benchmark's 145×97 dimensions, so no direct strip timing ratio is claimed.

Raw evidence: `audit-before-node.json` on Scratch, `audit-after-node.json` on Scratch, `audit-after-browser.json` on Scratch. Reproduction commands are in the [measurement guide](saw-planner-results.md#reproduction-and-coverage).

## Verification scope

The focused Rust suite contains 31 tests. Native smoke contains eight tests and WASM smoke contains 19, including legacy binding behavior. The repository gate covers formatting, TOML/text hygiene, all-target/all-feature Clippy, workspace tests and private-item Rust documentation. Binding release builds and runtime TypeScript checks exercise the new public API.

The planner remains bounded and heuristic on larger jobs. Complex layouts may still exhaust construction limits, and geometry enumeration is outside state accounting. No packing behavior, package publication or Subchecker integration is changed by this audit.

Raw run evidence is stored outside git under `/Volumes/Scratch/bin-packing/reports/release-v0.5.0-20261017/docs/two-d/saw-planner-data/`.
