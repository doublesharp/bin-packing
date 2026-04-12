# Best-Fit Decreasing (`best_fit_decreasing`)

`OneDAlgorithm::BestFitDecreasing` — the same decreasing-length construction
as [FFD](first-fit-decreasing.md), but instead of scanning bins in creation
order it picks the bin that leaves the smallest remaining gap after
placement. Still deterministic, still O(n²), no randomness.

## Mechanism

1. Expand `demands` into a flat list of `n` piece instances.
2. **Sort pieces by length, descending** (same as FFD).
3. For each piece in that order:
   1. Collect every currently-open bin where the piece fits, compute
      `remaining_after_placement = bin.remaining_length - kerf_delta`, and
      pick the bin minimizing the lexicographic tuple
      `(remaining_after_placement, stock.cost)`. The cost tiebreaker lets
      BFD prefer filling a cheaper bin when two bins would leave the same
      residual.
   2. If no bin fits, open a fresh bin via `choose_new_stock` (same
      multi-stock comparator as FFD — see
      [overview](README.md#multi-stock-selection)).
   3. If no fresh bin can be opened either, append to `unplaced`.

The returned `OneDSolution.metrics.iterations` is `1` and the `notes`
vector contains one line:
`"deterministic best-fit-decreasing construction"`.

## How it differs from FFD

FFD commits to the first bin that *fits*. BFD commits to the bin that fits
*tightest*. In the expected case where many bins are partially filled, BFD
closes bins faster and opens fewer new ones. On a classic cutting-stock
workload with mixed sizes close to half-capacity, BFD typically beats FFD
by 1–3 bins per hundred. On highly uniform workloads (everything is the
same size) they produce identical output because every fit is equally
tight.

BFD is also the seed that [local search](local-search.md) often starts
from — the local search compares the FFD and BFD constructions up front
and keeps whichever baseline is better before beginning its improvement
loop.

## Complexity

- **Time**: O(n²) — each of `n` pieces evaluates every open bin for the
  tight-fit comparison.
- **Space**: O(n).

## Ranking and solution fields

- `OneDSolution.algorithm = "best_fit_decreasing"`
- `exact = false`, `lower_bound = None`
- `metrics.iterations = 1`, all other metric counters `0`
- Layouts sorted by utilization descending, unplaced cuts sorted by length
  descending.

## When to use it

- As a **fast deterministic heuristic** when you want better packing than
  FFD without the cost of local search.
- When the workload is **dominated by pairs of mid-sized cuts** — BFD
  aggressively collects two-piece bins, which is often the shape that
  wastes the most capacity under FFD.
- As a **baseline for diagnosing local search regressions** — if local
  search is not producing a better solution than raw BFD, something is
  wrong with the improvement loop.

## When to avoid it

- When you have **very few distinct sizes** and a large quantity — the
  first-fit bin found is always a tight fit because every piece is the
  same size, so FFD and BFD produce identical output and FFD is a tiny
  bit faster.
- When you want **proven optimality** on a small instance. Use
  [column generation](column-generation.md) or [Auto](auto.md).

## Rust entry point

```rust
use bin_packing::one_d::{OneDAlgorithm, OneDOptions, OneDProblem, solve_1d};

let solution = solve_1d(
    problem,
    OneDOptions {
        algorithm: OneDAlgorithm::BestFitDecreasing,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/one_d/heuristics.rs`](../../crates/bin-packing/src/one_d/heuristics.rs)
(see `solve_bfd`, `pack_ordered`, `choose_existing_bin` — the
`PlacementStrategy::BestFit` arm is what distinguishes BFD from FFD).
