# Auto (`auto`)

`TwoDAlgorithm::Auto` — the default 2D algorithm. Runs an ensemble of
strong construction heuristics and returns the best candidate under the
2D solution comparator. When `TwoDOptions.guillotine_required` is set, the
ensemble is narrowed to guillotine-compatible algorithms only.

## Default ensemble (`guillotine_required = false`)

Auto runs the following ten constructions in order and keeps whichever is
best under `is_better_than`:

1. [`max_rects`](max-rects.md#max_rects--best-area-fit-baf) — Best-Area-Fit MaxRects. Seed the `best` slot.
2. [`max_rects_best_short_side_fit`](max-rects.md#max_rects_best_short_side_fit--best-short-side-fit-bssf) — BSSF MaxRects.
3. [`max_rects_best_long_side_fit`](max-rects.md#max_rects_best_long_side_fit--best-long-side-fit-blsf) — BLSF MaxRects.
4. [`max_rects_contact_point`](max-rects.md#max_rects_contact_point--contact-point) — Contact-point MaxRects.
5. [`skyline`](skyline.md#skyline--skyline-bottom-left) — Skyline Bottom-Left.
6. [`skyline_min_waste`](skyline.md#skyline_min_waste--skyline-minimum-waste) — Skyline Minimum-Waste.
7. [`best_fit_decreasing_height`](shelf.md#best_fit_decreasing_height--bfdh) — BFDH shelf.
8. [`guillotine_best_short_side_fit`](guillotine.md#guillotine_best_short_side_fit--bssf--beamboth) — Guillotine BSSF (beam-both).
9. [`guillotine_shorter_leftover_axis`](guillotine.md#guillotine_shorter_leftover_axis--baf--slas) — Guillotine BAF+SLAS.
10. [`multi_start`](multi-start.md) — Multistart MaxRects (honors `seed`).

The ensemble is deliberately curated to cover several failure modes:

- **MaxRects BAF** is the strongest general-purpose baseline.
- **BSSF** and **BLSF** bracket the workload by leftover-edge direction.
- **Contact-point** catches irregular workloads where the area-fit
  metrics don't produce tightly-hugging layouts.
- **Skyline and skyline-min-waste** catch workloads where a 1D-over-x
  representation outperforms 2D free-rect tracking.
- **BFDH** is the fast shelf baseline — a cheap safety net on simple
  workloads. Its row-wise layouts are guillotine-style, but the current
  solver reports `solution.guillotine = false` for shelf variants.
- **Two Guillotine variants** extend the above with non-MaxRects
  geometry, often winning on workloads with many similar-size items.
- **MultiStart** gives the ensemble access to non-greedy BAF runs that
  explore the tie-ordering neighborhood.

**Not in the default ensemble** (reachable only via explicit selection):

- [`max_rects_bottom_left`](max-rects.md#max_rects_bottom_left--bottom-left) — dominated by BAF / BSSF on bin count, kept for visualization purposes.
- [`guillotine`](guillotine.md#guillotine--best-area-fit--beamboth) (BAF+BeamBoth) — close to `guillotine_best_short_side_fit` in practice.
- [`guillotine_best_long_side_fit`](guillotine.md#guillotine_best_long_side_fit--blsf--beamboth), [`guillotine_longer_leftover_axis`](guillotine.md#guillotine_longer_leftover_axis--baf--llas), [`guillotine_min_area_split`](guillotine.md#guillotine_min_area_split--baf--minarea), [`guillotine_max_area_split`](guillotine.md#guillotine_max_area_split--baf--maxarea) — niche split heuristics; select explicitly if your workload needs them.
- [`next_fit_decreasing_height`](shelf.md#next_fit_decreasing_height--nfdh), [`first_fit_decreasing_height`](shelf.md#first_fit_decreasing_height--ffdh) — typically dominated by BFDH.

You can always bypass Auto and invoke these explicitly via
`TwoDAlgorithm::<Variant>`.

## Guillotine-required ensemble (`guillotine_required = true`)

When `TwoDOptions.guillotine_required = true`, Auto narrows its candidate
set to the full [Guillotine family](guillotine.md):

1. `guillotine` — BAF + BeamBoth.
2. `guillotine_best_short_side_fit` — BSSF + BeamBoth.
3. `guillotine_best_long_side_fit` — BLSF + BeamBoth.
4. `guillotine_shorter_leftover_axis` — BAF + SLAS.
5. `guillotine_longer_leftover_axis` — BAF + LLAS.
6. `guillotine_min_area_split` — BAF + MinArea.
7. `guillotine_max_area_split` — BAF + MaxArea.

All seven produce `TwoDSolution.guillotine = true`. The MaxRects,
Skyline, Shelf, and MultiStart families are skipped entirely — the
`guillotine_required` flag is a **candidate-set filter**, not a
post-construction check. Every candidate Auto ranks under the flag is
already guillotine-compatible by construction.

See [2D overview — `guillotine_required`](README.md#guillotine_required-and-twodsolutionguillotine)
for why the flag does not participate in the solution comparator.

## Ranking

Every candidate is compared against the current best via
`TwoDSolution::is_better_than`, which enforces the 2D lexicographic
comparator:

```
(unplaced.len(), sheet_count, total_waste_area, total_cost)
```

See [2D overview — ranking](README.md#solution-ranking). The comparator
is stable: if two candidates produce identical tuples, the earlier one
in the ensemble order wins.

## Options inherited from the ensemble members

- **`beam_width`** (default `8`) — passed to every Guillotine variant
  in the ensemble.
- **`multistart_runs`** (default `12`) — passed to `multi_start`. Not
  used by the single-run families.
- **`seed`** — passed to `multi_start`, which is the only randomized
  family in the ensemble. When `seed = None`, `multi_start` falls back
  to its internal constant so the Auto run is still reproducible
  across processes.

The `guillotine_required` flag is consumed directly by Auto itself to
route the dispatch path.

## Complexity

Auto's cost is the sum of its ensemble members. The default ensemble
runs ten constructions; the guillotine-required ensemble runs seven.
On a medium instance (several hundred items) Auto typically takes
tens to hundreds of milliseconds. For latency-sensitive callers that
want a single specific algorithm, bypass Auto and call the
explicitly-chosen `TwoDAlgorithm::<Variant>` directly.

## When to use it

- As the **default**. `TwoDOptions::default().algorithm` is Auto
  because Auto is the safest choice across all workload shapes: it
  runs every strong variant and returns the best result.
- When you want **the best answer the library can produce** without
  having to profile your workload against each variant.
- When you need **guillotine compatibility** — set
  `guillotine_required = true` and Auto runs the full Guillotine
  ensemble for you.

## When to bypass it

- When you have **profiled your workload** and know a specific variant
  is consistently better. Bypassing Auto saves 5-10x the runtime.
- When you need **hard latency guarantees**. Auto's cost is the sum of
  its members, so worst-case latency is the slowest ensemble member
  times the ensemble size. A direct call to [Shelf BFDH](shelf.md) or
  a single MaxRects variant gives a predictable ceiling.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDOptions, TwoDProblem, solve_2d};

// Default: Auto is selected for you.
let solution = solve_2d(problem, TwoDOptions::default())?;

// Auto with guillotine-required mode:
let solution = solve_2d(
    problem,
    TwoDOptions {
        guillotine_required: true,
        seed: Some(42),
        ..Default::default()
    },
)?;
assert!(solution.guillotine);
```

Source: [`crates/bin-packing/src/two_d/mod.rs`](../../crates/bin-packing/src/two_d/mod.rs)
(see `solve_2d`, `solve_auto`, `solve_auto_guillotine`).
