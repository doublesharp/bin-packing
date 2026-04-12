# Multistart Local Search (`local_search`)

`OneDAlgorithm::LocalSearch` — a randomized multistart construction with a
bin-elimination repair pass. Seeds from both [FFD](first-fit-decreasing.md)
and [BFD](best-fit-decreasing.md), then runs `multistart_runs` restarts on
jittered piece orderings, and attempts to eliminate the lightest bins by
redistributing their contents. Reproducible under `OneDOptions.seed`.

## Mechanism

### Phase 1 — baseline

1. Expand demands into `baseline_pieces`, sorted descending.
2. Run FFD against `baseline_pieces`, producing a baseline solution.
3. Run BFD against `baseline_pieces`.
4. Keep whichever baseline is better under the 1D comparator (see
   [overview](README.md#solution-ranking)). Record whether FFD or BFD won
   in a persistent `"started from ..."` note.

### Phase 2 — multistart improvement loop

For `run in 0..multistart_runs.max(1)`:

1. **Perturb the piece order.** `perturb_piece_order` does three things:
   1. Shuffle the piece list uniformly.
   2. Re-sort descending. This restores the decreasing-size prior that
      makes construction heuristics work, but on a freshly shuffled tie
      ordering — two pieces of identical length can now end up in either
      order, whereas the raw baseline always uses `demand_index` as the
      tiebreaker.
   3. Do `max(n / 4, 1)` random swaps between piece pairs whose lengths
      differ by at most `5`. This injects near-miss reorderings that a
      pure shuffle-then-sort wouldn't produce.
2. **Pack** the perturbed order with best-fit on even runs and first-fit
   on odd runs, alternating between the two placement strategies. This
   turns the single `run` index into two independent trajectories.
3. If the packing placed every piece (no inventory-induced overflow),
   run the **bin-elimination repair pass** (see Phase 3).
4. Compare the candidate against the running best under the 1D comparator.
   If it improves, swap it in and record an `"accepted improved run N"`
   note.

### Phase 3 — bin-elimination repair

`eliminate_bins` runs up to `improvement_rounds` rounds. Each round:

1. Sort the current bins by piece count ascending — the bin with the
   fewest pieces is the easiest target for elimination.
2. For each candidate bin in that order:
   1. Clone the current bin list with the candidate removed.
   2. `try_insert_without_opening` attempts to reinsert every piece from
      the removed bin into the reduced list, using tightest-fit
      best-fit placement, **without opening any new bins**.
   3. If every piece lands, commit the reduced list and break out of
      the inner loop (one elimination per round).
3. If a full pass finds no bin that can be eliminated, stop early. The
   repair is monotone: `bin_count` never increases.

This is a classic cutting-stock repair move: the expensive slot is always
"can I redistribute this bin's contents into the others", and the
decreasing-length ordering inside `try_insert_without_opening` gives each
piece the best shot at finding a tight slot.

## Randomness and reproducibility

- The RNG is `rand::rngs::SmallRng`.
- `OneDOptions.seed = Some(u64)` is plumbed through `seeded_rng`.
- `OneDOptions.seed = None` falls back to the fixed constant
  `0x4249_4E50_4143_4B30` (ASCII `"BINPACK0"`), so every run is still
  reproducible across processes — the no-seed path is *defaulted*, not
  *nondeterministic*.
- Identical `(problem, options, seed)` triples produce bit-identical
  output.

## Complexity

- **Time**: O(`multistart_runs.max(1)` × `n²`) for the construction phase,
  plus O(`multistart_runs.max(1)` × `improvement_rounds` × n²) worst case
  for the repair pass. In practice the repair loop converges quickly because
  most eliminations become infeasible after two or three passes.
- **Space**: O(n) — the bin list is repeatedly cloned and discarded but
  the live size is bounded by the current solution's bin count.

## Ranking and solution fields

- `OneDSolution.algorithm = "local_search"`
- `exact = false`, `lower_bound = None`
- `metrics.iterations = multistart_runs.max(1) + 2` (the `+2`
  accounts for the FFD and BFD baselines)
- `metrics.notes` always contains the `"started from ..."` baseline note
  and `"combines multistart reorderings with a bin-elimination repair
  pass"`, plus one `"accepted improved run N"` line per improvement that
  was adopted.
- Layouts sorted by utilization descending, unplaced cuts sorted by
  length descending.

## Options that matter

- **`multistart_runs`** (default `16`) — number of perturbed restarts.
- **`improvement_rounds`** (default `24`) — upper bound on
  bin-elimination passes per successful restart. The pass breaks early
  once it can no longer eliminate any bin.
- **`seed`** — optional `u64` for reproducible output.

## When to use it

- When you want **better packing than FFD / BFD** without paying the
  setup cost of the exact backend — typical improvements over BFD are 1–5
  bins per hundred on mixed workloads.
- When you want **reproducible nondeterminism** — pass a `seed` and you
  get the same output forever, but different seeds can recover different
  local optima that a pure-greedy approach would miss.
- On **moderately large instances** (100–10,000 pieces). The algorithm
  scales smoothly with `multistart_runs` so you can tune the time budget.

## When to avoid it

- On **tiny instances** (≤ 30 pieces with ≤ 10 distinct types) where
  [column generation](column-generation.md) can prove optimality
  outright.
- On **huge instances** (`n > 100,000`) where even
  `multistart_runs.max(1) × n²` is prohibitive. Drop to raw
  [FFD](first-fit-decreasing.md) or
  [BFD](best-fit-decreasing.md) and call them directly.

## Rust entry point

```rust
use bin_packing::one_d::{OneDAlgorithm, OneDOptions, OneDProblem, solve_1d};

let solution = solve_1d(
    problem,
    OneDOptions {
        algorithm: OneDAlgorithm::LocalSearch,
        multistart_runs: 32,
        improvement_rounds: 50,
        seed: Some(42),
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/one_d/heuristics.rs`](../../crates/bin-packing/src/one_d/heuristics.rs)
(see `solve_local_search`, `perturb_piece_order`, `eliminate_bins`,
`try_insert_without_opening`).
