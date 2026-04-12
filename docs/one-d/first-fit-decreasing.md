# First-Fit Decreasing (`first_fit_decreasing`)

`OneDAlgorithm::FirstFitDecreasing` — classic FFD construction for 1D
cutting stock. Deterministic, O(n²), no randomness, no configuration knobs.
It is the baseline that every other 1D algorithm is measured against.

## Mechanism

1. Expand `demands` into a flat list of `n` piece instances (one per unit of
   `quantity`).
2. **Sort pieces by length, descending**, with `demand_index` as a stable
   tiebreaker so two same-length cuts from different demands always come
   out in a fixed order.
3. For each piece in that order:
   1. Scan the currently-open bins in creation order and place the piece in
      **the first bin that can accept it** after kerf accounting. A bin
      accepts a piece iff
      `bin.used_length + (piece.length + kerf_if_not_first_cut) <= stock.usable_length`.
   2. If no existing bin accepts the piece, open a fresh bin from the
      `choose_new_stock` comparator (tightest-fit stock first, ties broken
      by cost then raw length — see [overview](README.md#multi-stock-selection)).
      The piece is then placed in that fresh bin.
   3. If no eligible stock type is available (because every candidate is
      either too short or has exhausted its `available` inventory cap),
      the piece is appended to `unplaced`.

The returned `OneDSolution.metrics.iterations` is always `1`. The
`notes` vector contains one line: `"deterministic descending construction"`.

## Placement invariants

- **Pieces are tried in decreasing length order**, so the largest cuts get
  their choice of bin first. This is what the "Decreasing" in FFD buys you:
  without the sort, the algorithm is First-Fit, which has a worse
  worst-case ratio.
- **Kerf is charged between adjacent cuts**, not before the first cut, so a
  bin holding one piece uses `piece.length` (no kerf), two pieces use
  `piece1.length + kerf + piece2.length`, etc. This matches physical saw
  loss.
- **Trim is deducted up front** via `usable_length = length - trim`. Every
  capacity check uses the usable length, never the raw stock length.
- A piece that reaches `choose_new_stock` is guaranteed to fit the new bin
  (the candidate stock was filtered by `usable_length >= piece.length`).
  If the fresh bin somehow rejects the piece anyway, `solve_1d` raises
  `BinPackingError::Unsupported("internal invariant violation...")` rather
  than a misleading `Infeasible1D`. This path is guarded by `debug_assert!`
  and is not expected to fire in practice.

## Complexity

- **Time**: O(n²) worst case — each of `n` pieces scans up to `O(n)`
  existing bins. In practice the inner scan is much shorter because bins
  close once they're tightly filled.
- **Space**: O(n) for the expanded piece list and bin state.

## Ranking and solution fields

- `OneDSolution.algorithm = "first_fit_decreasing"`
- `exact = false`, `lower_bound = None`
- `metrics.iterations = 1`, `metrics.generated_patterns = 0`,
  `metrics.enumerated_patterns = 0`, `metrics.explored_states = 0`
- Layouts are returned sorted by utilization descending. Unplaced cuts
  (nonempty only when inventory caps are hit) are sorted by length
  descending.

## When to use it

- As a **fast deterministic baseline** for sanity-checking other solvers.
- When you need **stable output** across runs with no `seed` wiring
  required and don't care about squeezing the last 1–3% out of the bin
  count.
- As the **warm start** for local search — in the default Auto mode,
  `local_search` seeds from both FFD and BFD.
- On very large instances (`n > 10_000`) where even O(n²) is borderline,
  consider calling FFD directly rather than letting Auto run the full
  ensemble. FFD's worst-case approximation ratio against the optimal bin
  count is `11/9 · OPT + 6/9`, which is often close enough in practice.

## When to avoid it

- When you need the **tightest possible bin count** on a small instance
  (N ≤ 100). Use [column generation](column-generation.md) or the default
  [Auto](auto.md) mode, which escalates for you.
- When your workload has many **mixed sizes close to half-capacity**.
  BFD typically wins on that shape because it collects two-piece bins more
  aggressively.

## Rust entry point

```rust
use bin_packing::one_d::{OneDAlgorithm, OneDOptions, OneDProblem, solve_1d};

let solution = solve_1d(
    problem,
    OneDOptions {
        algorithm: OneDAlgorithm::FirstFitDecreasing,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/one_d/heuristics.rs`](../../crates/bin-packing/src/one_d/heuristics.rs)
(see `solve_ffd`, `pack_ordered`, `choose_existing_bin`).
