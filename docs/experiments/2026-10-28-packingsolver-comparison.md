# Native expansion and PackingSolver comparison

This is the historical first-batch report. Native implementations of its four
remaining capability groups are documented in the
[completion plan](../packingsolver-completion-plan.md).

This expansion adds native Rust cost-first 1D search, constrained 2D sheet
packing, payload-aware 3D packing, conservative 2D lower bounds and adaptive
selection. Node and dimension-specific WebAssembly expose the new APIs. No
production dependency or C++ runtime was added. Existing solver entry points and
their ranking remain unchanged.

This first batch did **not** establish complete PackingSolver parity or a
universal performance lead. At that point irregular shapes, advanced loading,
knapsack/open-dimension objectives and staged restrictions remained open; their
native implementations and qualification are recorded in the
[completion report](2026-10-28-packingsolver-completion.md).

## Comparison method

Reference: PackingSolver `376b8d1cc4aed5b171bfe1219c037c661faaa7cf`, native release
rectangle and box executables, automatic algorithm selection in
`not-anytime-sequential` mode, explicit `--linear-programming-solver highs`, and
a two-second limit. The initial build included HiGHS but defaulted to absent
CLP; those prerequisite failures were excluded from the final comparison.

The 18 cases contain seven targeted cases and eleven synthetic held-out cases.
Two previously observed losses became explicitly labeled targeted regressions;
four fresh seeds were added before the final run. Inputs use a single stock,
zero kerf, no defects/trims/weights/support constraints and a bin-count objective.
For these inputs, equal counts also fix material cost and total empty measure.
New physical constraints and 1D pricing are separately covered by regressions
and independent tiny-instance oracles, rather than this cross-engine benchmark.

Each engine/variant has one warmup and five measured samples, run serially.
Rust uses one Rayon thread, beam width eight and a 512-state constrained budget;
other controls and all normalized input hashes are in the raw report. Internal
solve time is separate from process/serialization overhead. Budgets differ:
these are completed configured runs, not measurements of time to equal quality.
All output certificates are independently validated. All five reference counts
were stable for every case, and source/binary hashes stayed unchanged.

The host has 12 logical CPUs. Load averages started at 9.78/8.29/9.92 and ended
at 6.86/7.72/9.64, so exact timing ratios require a quieter repeat before they
are treated as stable performance claims.

## Count results

Fewer/more is relative to PackingSolver under the stated limits.

| Rust API | Cases | Fewer bins | Same count | More bins |
| --- | ---: | ---: | ---: | ---: |
| 2D ordinary Auto | 10 | 0 | 7 | 3 |
| 2D adaptive | 10 | 0 | 7 | 3 |
| 2D constrained | 10 | 0 | 9 | 1 |
| 3D ordinary Auto | 8 | 2 | 6 | 0 |
| 3D constrained | 8 | 2 | 6 | 0 |

The constrained 2D repair improves the elongated regression from two sheets to
one at the same 512-state budget. Fresh held-out `2d-seed-947` also improves
from ordinary Auto's two sheets to one, matching the reference. The 3D
constructor recovers ordinary Auto's one-bin result on the prior constrained
regression without calling Auto or increasing its successor budget.

The remaining held-out loss is `2d-seed-421`: every tested Rust variant uses
three sheets while PackingSolver uses two. A separate post-comparison budget
diagnostic still uses three at 4,000 and 100,000 states, so the loss cannot be
explained away by the original small budget. The two 3D count wins are against
reference runs reaching their two-second cutoff; they are not unrestricted
optimality or time-to-quality comparisons.

## Example configured runtimes

These examples have matching final counts. Times are five-sample internal
medians in milliseconds, excluding compilation and JSON/process overhead.

| Case | Rust API | Bins | Rust ms | PackingSolver ms |
| --- | --- | ---: | ---: | ---: |
| 2D homogeneous | adaptive | 2 | 0.062 | 2.560 |
| 2D elongated | constrained | 1 | 0.620 | 153.759 |
| 2D seed 947 | constrained | 1 | 0.400 | 434.538 |
| 3D seed 431 | constrained | 1 | 0.034 | 2.269 |

All cases, samples, slower/poorer results, commands, certificates and source
hashes are retained under
`/Volumes/Scratch/bin-packing/reports/packingsolver-expansion-20261028/`.
The final report is `comparison-highs-20261028.json`; the separate budget
diagnostics are `budget-421-4000.json` and `budget-421-100000.json`.

## Verification and reproduction

Final checks passed: `check-quality.sh` (461 Rust tests and 20 Python tests),
eight Rust/nine WASM feature combinations, fuzz-target compilation/clippy,
rebuilt native/WASM packages (45 tests), 13 declaration checks, 300 generated
cases/1,800 solver calls, four real Chromium variants, two doctests and Cargo
package verification. One pre-existing optional benchmark test stays ignored by
the normal test gate. Independent source review found no unresolved P1/P2.

Build `packingsolver_compare` in release mode through the Scratch wrapper.
`scripts/compare-packingsolver.py --help` lists the explicit Rust/reference
binary and clean pinned-source paths. Use `--repeats 5 --lp-solver highs
--time-limit 2 --max-states 512` and a new `--output` path under Scratch.
Build the reference's `PackingSolver_rectangle_main` and
`PackingSolver_box_main` CMake targets with HiGHS enabled; the executable names
are `packingsolver_rectangle` and `packingsolver_box`.

Task-owned reference sources/builds and temporary test-tool downloads were
removed after ownership/open-file checks. Reproduction requires rebuilding the
pinned reference. Final receipts and the repository's warm build/cache remain.

Larger browser payloads are a tradeoff: final uncompressed WASM assets are about
309 KiB (1D), 637 KiB (2D), 663 KiB (3D) and 1.16 MiB (combined), excluding JS
wrappers and transfer compression. These figures include the new entry points.
