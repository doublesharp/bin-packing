# Rotation Search (`rotation_search`)

`TwoDAlgorithm::RotationSearch` — an exhaustive (or sampled) rotation
assignment search over rotatable demand types. Enumerates all 2^k
rotation assignments for k rotatable demand types, or samples
`multistart_runs` random assignments when k exceeds
`auto_rotation_search_max_types`. Each assignment fixes rotations and
packs via [MaxRects Best-Area-Fit](max-rects.md#max_rects--best-area-fit-baf).
Reproducible under `TwoDOptions.seed`.

## Mechanism

1. **Identify rotatable demand types.** A demand type is rotatable when
   `can_rotate == true` AND `width != height` (square items gain nothing
   from rotation). Let k = the number of such types.
2. **Choose enumeration mode:**
   - If `k <= auto_rotation_search_max_types` (default `16`) and
     `k <= 63`, enumerate all 2^k rotation assignments exhaustively.
   - Otherwise, sample `multistart_runs` random assignments.
3. **For each rotation assignment** (a bitmask over the k types):
   1. **Apply the rotation mask.** For each rotatable type, bit `i = 1`
      means swap width/height and lock `can_rotate = false`; bit `i = 0`
      means keep the declared orientation and lock `can_rotate = false`.
      Non-rotatable types preserve their original `can_rotate` flag.
   2. **Sort items** by area descending (the standard MaxRects baseline).
   3. **Run MaxRects Best-Area-Fit** on the fixed-orientation item list.
4. **Return the best candidate** under the 2D solution comparator.
   `metrics.notes` records whether the search was exhaustive or sampled,
   plus the number of assignments evaluated.

## Why rotation assignment matters

Deterministic MaxRects with `can_rotate = true` evaluates both
orientations per item per candidate placement. This greedy per-item
choice is locally optimal but globally short-sighted: committing one
item to a rotated orientation early may cascade into worse free-rect
splits for all subsequent items.

Rotation search separates the rotation decision from the placement
decision. By fixing all rotations upfront and running the packer with
`can_rotate = false`, the packer sees a single deterministic item shape
list. Searching across all 2^k assignments (or a large random sample)
explores the global rotation landscape that greedy per-placement
rotation cannot reach.

## Exhaustive vs sampled mode

- **Exhaustive** (`k <= auto_rotation_search_max_types`, default 16):
  evaluates every possible combination. For k = 16 this is 65,536
  assignments — each is a fast MaxRects construction, so the total
  time is manageable. Guarantees the globally best rotation assignment
  for the BAF scoring function.
- **Sampled** (`k > auto_rotation_search_max_types`): generates
  `multistart_runs` random masks via `SmallRng` seeded from
  `options.seed` (or a fixed constant). Does not guarantee optimality
  but explores the rotation landscape cheaply on workloads with many
  rotatable types.

## Options

- **`auto_rotation_search_max_types`** (default `16`) — threshold for
  exhaustive vs sampled mode. Also controls whether [Auto](auto.md)
  includes rotation search as a candidate.
- **`multistart_runs`** (default `12`) — number of random samples in
  sampled mode. Ignored in exhaustive mode.
- **`seed`** — `Option<u64>`. Used in sampled mode to seed the RNG.
  `None` falls back to the built-in constant `0x524F_5441_5445_5253`;
  `Some(s)` makes the run fully reproducible across processes.

## Complexity

- **Exhaustive mode:** O(2^k × n²) where n = total expanded items and
  k = rotatable demand types. Each assignment runs a full MaxRects BAF
  construction in O(n²).
- **Sampled mode:** O(multistart_runs × n²). Linear in the sample
  count.
- **Space:** O(n) live at any one time — each trial's state is
  discarded once compared against `best`.

## Output

- `TwoDSolution.algorithm = "rotation_search"`.
- `guillotine = false` — MaxRects BAF does not produce
  guillotine-compatible layouts.
- `metrics.notes` contains the mode description (exhaustive or sampled)
  and the number of assignments evaluated.

## When to use it

- When your workload has **many rotatable rectangular items** and you
  suspect that the greedy per-placement rotation choice is suboptimal.
- When you want to **explore the rotation landscape** more thoroughly
  than MaxRects or MultiStart can on their own.
- As part of **Auto**, which includes rotation search in its default
  ensemble.

## When to avoid it

- When your workload has **few or no rotatable items** — the search
  space is trivial and rotation search reduces to a single MaxRects run.
- When **all items are square** — rotation has no effect and the search
  degenerates.
- When you need **guillotine cuts**. Use [Auto](auto.md) with
  `guillotine_required = true` or call a [Guillotine](guillotine.md)
  variant directly.
- When you need **the absolute fastest path**. Call MaxRects BAF
  directly.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::RotationSearch,
        auto_rotation_search_max_types: 16,
        seed: Some(42),
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/two_d/rotation_search.rs`](../../crates/bin-packing/src/two_d/rotation_search.rs)
(see `solve_rotation_search`, `apply_rotation_mask`, `rotatable_demand_indices`).
