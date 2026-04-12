# Multistart MaxRects (`multi_start`)

`TwoDAlgorithm::MultiStart` — a randomized multistart meta-strategy over
the [MaxRects Best-Area-Fit](max-rects.md#max_rects--best-area-fit-baf)
construction. Runs `multistart_runs` trials under different tie orderings
of the input items, with a minimum of one perturbed trial, and returns the
best candidate under the 2D solution comparator. Reproducible under
`TwoDOptions.seed`.

## Mechanism

1. **Expand and sort items** deterministically by area descending (the
   standard MaxRects baseline).
2. **Run the baseline** — one MaxRects Best-Area-Fit construction against
   the deterministic ordering. This becomes the initial `best`.
3. **Seed the RNG.** If `options.seed = Some(s)`, use
   `SmallRng::seed_from_u64(s)`. Otherwise fall back to the fixed constant
   `0x4D41_5852_4543_5453`
   (ASCII `"MAXRECTS"`), so the no-seed path is still reproducible.
4. **For `run in 0..multistart_runs.max(1)`:**
   1. **Generate a perturbed ordering** via `multistart_ordered_items`.
      The sort is stable on `(area desc, longer_side desc)` — the
      deterministic primary keys — but breaks ties with a per-item RNG
      key. Items with identical area and identical longest-side score
      can then be placed in any order. The RNG keying is what produces
      different orderings across runs.
   2. **Run MaxRects Best-Area-Fit** on the perturbed ordering.
   3. **Compare** the candidate against `best` via
      `is_better_than` and swap if the candidate wins.
5. **Return the best candidate.** `metrics.notes` is augmented with
   `"multistart randomizes tie ordering among similarly ranked items"`
   and `"multistart kept the best candidate"`.

## Why tie-order matters

Deterministic MaxRects builds a fixed sort order based on item area
descending with a length tiebreaker. Two items with equal area and
equal longest-side are placed in whatever order `demand_index`
produces. On mixed-size workloads this is rare and the tie-breaking
doesn't matter much; on uniform or semi-uniform workloads it matters
a lot, because a single early placement decision cascades into a
sequence of free-rect splits that fully determines the rest of the
layout.

MultiStart exploits this: by randomizing the tie order among equally-
ranked items, it explores a neighborhood of the construction tree
that a purely greedy MaxRects never reaches. Typical improvements
over single-shot BAF are 1–3 sheets per hundred on workloads with
many near-identical items.

## What it does NOT randomize

- **Item sizes** are not modified.
- **The primary sort key** (area descending) is preserved — MultiStart
  is still a decreasing-size construction.
- **The BAF scoring comparator** is fixed — MultiStart does not cycle
  through BSSF/BLSF/contact-point. Use [Auto](auto.md) if you want
  cross-scoring-strategy exploration.
- **Free-rect split and pruning logic** are unchanged from the
  underlying BAF construction.

In short: MultiStart is **BAF with jittered tie-breaking**, not a
new placement strategy.

## Options

- **`multistart_runs`** (default `12`) — number of perturbed trials.
  The baseline deterministic run is always executed, and the loop runs
  `multistart_runs.max(1)` perturbed trials.
- **`seed`** — `Option<u64>`. `None` falls back to the built-in
  constant; `Some(s)` makes the run fully reproducible across
  processes.

## Complexity

- **Per trial:** O(n²) for the underlying MaxRects BAF construction
  (see [MaxRects complexity](max-rects.md#complexity)).
- **Total:** O((multistart_runs.max(1) + 1) × n²). Linear in
  `multistart_runs`, so it scales smoothly with the time budget.
- **Space:** O(n) live at any one time — each trial's state is
  discarded once compared against `best`.

## Output

- `TwoDSolution.algorithm = "multi_start"`.
- `guillotine = false` — MaxRects BAF does not produce
  guillotine-compatible layouts, and MultiStart does not change that.
- `metrics.iterations` reflects the winning candidate's construction
  counter, not the total number of constructions attempted.
- `metrics.explored_states = 0`.
- `metrics.notes` contains the randomization description.

## When to use it

- When you want **better MaxRects output than single-shot BAF** and
  are willing to run 10–30 constructions.
- When your workload has **many near-identical items** — this is
  where tie-breaking matters most and where MultiStart delivers the
  biggest improvements.
- When you want **reproducible but non-greedy output** — pass a
  `seed` for deterministic runs across processes.

## When to avoid it

- When the workload has **highly variable item sizes**. The tie
  pool is small, so the randomization does not explore much of the
  construction tree and you pay the multistart overhead for no gain.
- When you need **guillotine cuts**. Use [Auto](auto.md) with
  `guillotine_required = true` (which routes to the Guillotine
  family) or call a [Guillotine](guillotine.md) variant directly.
- When you need **the absolute fastest path**. Call MaxRects BAF
  directly.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::MultiStart,
        multistart_runs: 32,
        seed: Some(42),
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/two_d/maxrects.rs`](../../crates/bin-packing/src/two_d/maxrects.rs)
(see `solve_multistart`, `multistart_ordered_items`, `pack_with_order`).
