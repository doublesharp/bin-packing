# Native PackingSolver expansion completion

The five remaining implementation areas are complete in Rust, with additive
Node and WebAssembly APIs. PackingSolver remains a pinned reference executable;
the production library has no PackingSolver or C++ runtime backend.

Subsequent [confidence checks](2026-10-28-confidence.md) add regression tests,
measure production coverage and repair strict polygon-hole validation.

| Area | Implemented behavior |
| --- | --- |
| [Irregular nesting](../irregular.md) | Concave polygons, holes, circular contours, rigid components, irregular bins/defects, legal rotations/reflection and clearances |
| [Stack loading](../three-d/loading.md) | Compatible upright columns, nesting, stack/weight/density limits, payload, exact two-support axle reactions and unloading groups |
| [Explicit objectives](../objectives.md) | Count, material cost, profit/knapsack, feasibility and open dimensions, with mandatory quantities and source-indexed accounting |
| [Staged cutting](../two-d/staged.md) | Authoritative physical trees, stage/cut modes, axes, strip distances, cut counts, kerf, trims, defects and production groups |
| [Packing quality](../two-d/constrained.md) | Dense mixed rectangles recover two sheets within the unchanged default 4,000-state budget, including a ten-times-scaled copy |

The dense case uses 15 parts with 840 total area on 24 by 18 sheets. The previous
implementation used three sheets at both 4,000 and 100,000 states. The new native
repair assigns source quantities between two sheets, seeds repeated-item blocks,
and runs bounded canonical single-sheet searches. It preserves the original
incumbent, checks both inventories and validates the combined physical result.
GCD normalization preserves behavior when the job's unit scale changes.

Source identity, geometric and machine constraints, work limits, approximation
and proof fields are documented with each API. Curves use a conservative envelope
and finite angular samples; loading uses upright equal-footprint columns and a
generic two-support model. Staged trees start after border preparation. These
are explicit bounded models, not a claim of every PackingSolver model option or
universal performance superiority.

## Final verification

`./scripts/check-quality.sh` passes 536 Rust tests and 37 Python tests, rustfmt,
Taplo, typos, strict workspace/all-target/all-feature Clippy, and rustdoc. One
pre-existing optional benchmark test remains ignored. The metadata gate now
checks the configured TOML files rather than treating the directory itself as
a file; generated local Cargo configuration is excluded.

`./scripts/check-features.sh` passes 10 isolated Rust/Node and 12 WASM feature
combinations, including irregular-only builds, plus fuzz-target formatting and
Clippy. Both Rust doctests pass. Cargo packages and verifies the 87-file crate
archive without publishing it.

`./scripts/check-bindings.sh` rebuilds the native package and all 15 WASM targets.
Native tarball loading, 15 packed WASM entry points, 16 TypeScript declaration
contracts, 300 generated cases/1,800 solver calls and five actual Chromium
variants pass. The browser is Chromium 141.0.7390.37; TypeScript 5.9.3 and
Playwright 1.56.1 are pinned disposable test tools on Scratch. Portable contracts
include profit reversal, physical cut reconstruction, nested loading/axle
readback, concave interlocking, strict numeric/constraint failures and the dense
two-sheet regression.

Runtime qualification is on the macOS ARM native host and Chromium; it does not
constitute a run on every native platform or browser. Uncompressed web WASM
modules are approximately 380 KiB for 1D, 873 KiB for 2D, 911 KiB for 3D,
753 KiB for irregular-only and 2.25 MiB combined, excluding JavaScript and
transfer compression. Dimension-specific imports load only the selected model.

Focused independent evidence includes 144 exhaustive finite knapsack cases across
the three regular dimensions, 108 stack-capacity oracle cases and 84 independently
calculated axle cases. Independent review found and rechecked two P2 fixes:
oversized cutting-group predecessors and omitted irregular sampling flags. The
final scoped review has no open material findings. Optimized 1D results omit a
selected subset's misleading full-job procurement estimate; ordinary 1D behavior
is preserved.

## Reference comparison method

The clean reference is PackingSolver
`376b8d1cc4aed5b171bfe1219c037c661faaa7cf`, built in release mode with HiGHS.
Commands explicitly select `--linear-programming-solver highs`,
`not-anytime-sequential` and a two-second reference limit. All runs are serial
with one untimed warmup. Native geometry and reference certificates are checked
independently. Source, normalized input and executable hashes, commands, all
samples, host load and full certificates remain on Scratch.

The regular suite contains 22 fixed synthetic/targeted cases plus eight public
geometry cases from Beasley 1985, Christofides 1977, Loh 1992 and Ivancic 1989. Public
sizes, quantities and explicit rotations are preserved; unit-price unlimited
stock and a bin-count objective form the common normalized problem. These are
not published scores for those collections' original objectives. Seed 421 is a
targeted regression; four fresh seeds were specified before final measurements.
There are five measured samples per regular case and a 4,000-state constrained
budget.

Fourteen additional targeted cases compare material cost, profit, feasibility,
open X, exact staged cuts and nested-column loading with three measured samples.
The 24-case [irregular report](2026-10-28-irregular-comparison.md) separately covers
14 targeted shapes and 10 unchanged public fixtures with three samples.

Internal solver medians exclude parsing, serialization and process startup.
Native calls include cloning and native validation; reference timing uses its
own `Time` field. Native repeats share a process; reference samples launch fresh
CLI processes. Process wall times are also retained. Effort limits and timer
lifecycle differ, and time to equal quality is not measured.

An incomplete reference incumbent remains a partial result with explicit source
shortfalls. Its geometry and summary are validated, including an empty certificate
when no item was packed before the cutoff. Such samples do not participate in
bin-count comparisons and are never interpreted as zero-bin optima.

## Measured outcomes

All 30 regular and 14 capability cases validate, with unchanged native source and
executable hashes during each run. Reference counts were stable across the five
measured regular repetitions. Fewer/same/more below compare complete packings
in every reference repetition; incomplete reference samples are separate.

| Regular Rust API | Cases | Fewer bins | Same count | More bins | Reference incomplete |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2D ordinary Auto | 16 | 0 | 13 | 3 | 0 |
| 2D adaptive | 16 | 0 | 13 | 3 | 0 |
| 2D constrained | 16 | 0 | 15 | 1 | 0 |
| 3D ordinary Auto | 14 | 2 | 10 | 1 | 1 |
| 3D constrained | 14 | 2 | 10 | 1 | 1 |

The constrained 2D loss is public Beasley `gcut2`: seven sheets versus six. The
3D loss is public Ivancic `thpack9.txt_2`: ten bins versus nine. Public Loh
`thpack8.txt_2` packs all 200 items into two bins in Rust; the reference has no
complete incumbent in any of its five measured two-second runs. That is a
completion difference, not a two-versus-zero bin-count win. These public losses
were retained without tuning the implementation after inspecting them.

Thirteen of 14 targeted capability cases match the reference primary value in
every measured repetition. In the payload-and-nesting case, Rust uses two bins
while the reference uses three; both certificates satisfy the same density,
nesting, stack-count, weight-above and payload constraints.

All 24 irregular cases also validate: primary quality matches on 23, while the
public open-X case remains wider in Rust. Among equal-quality irregular cases,
the native measured median is lower on eight and higher on fifteen. The detailed
irregular report preserves every case and the exact comparison controls.

The following are internal medians in milliseconds. These examples have matching
final primary quality and include a case where Rust is slower.

| Case | Rust API | Rust ms | Reference ms |
| --- | --- | ---: | ---: |
| Dense seed 421, two sheets | 2D constrained | 3.492 | 602.794 |
| Fractional material cost, cost 0.7 | 1D minimum cost | 0.062 | 0.201 |
| Exact three-stage pattern, one sheet | Staged 2D | 1.296 | 0.514 |
| Nested columns, one bin | Loading 3D | 0.325 | 3.714 |

The host has 12 logical CPUs. Its one-minute load average was 34.50 at regular-suite
start and 31.75 at the end. Exact runtime ratios therefore remain observations
under load; a quiet repeat is required before treating them as stable speed
claims. The separate irregular report retains its open-X quality loss and slower
curved cases. This expansion does not establish universal superiority.

The qualified regular receipt is `regular-final-v2.json`; the capability receipt
is `capability-final.json`. The earlier `regular-final.json` is retained as an
unqualified diagnostic: its original strict validator rejected empty cutoff
incumbents. A dedicated regression now distinguishes valid partial accounting
from a complete packing and verifies reported unpacked quantities.

## Reproduction and evidence

Run the release `packingsolver_compare` example through the Scratch wrapper.
The three comparison scripts provide `--help` for explicit native/reference
paths, output destinations and work/sample controls:

- `scripts/compare-packingsolver.py` for regular geometry and public data.
- `scripts/compare-capabilities-packingsolver.py` for objectives, trees and loads.
- `scripts/compare-irregular-packingsolver.py` for original curved material.

Use `python3 -B` for comparison-script invocations so imported validators do not
write disposable bytecode into the source checkout.

Final gate logs and regular/capability reports are retained under
`/Volumes/Scratch/bin-packing/reports/packingsolver-completion-20261028/`.
Irregular receipts are under
`/Volumes/Scratch/bin-packing/reports/irregular-completion-20261028/`.
Task-owned reference builds/clones, browser, downloaded test tools and superseded
prototypes were removed after ownership/open-file checks. Final receipts and
the checkout's shared warm build/cache remain. Reproduction rebuilds the pinned
reference and installs the tool versions above.
