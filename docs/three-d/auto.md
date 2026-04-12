# Auto (`auto`)

`ThreeDAlgorithm::Auto` runs a tiered ensemble of 3D algorithms and returns
the best successful candidate under `ThreeDSolution::is_better_than`. If every
candidate errors, Auto propagates the last error it observed.

## Tier 1

Tier 1 always runs these deterministic constructions in order:

1. [`extreme_points`](extreme-points.md#extreme_points)
2. [`extreme_points_residual_space`](extreme-points.md#extreme_points_residual_space)
3. [`extreme_points_contact_point`](extreme-points.md#extreme_points_contact_point)
4. [`guillotine_3d`](guillotine.md#guillotine_3d)
5. [`layer_building`](layer-building.md#layer_building)
6. [`first_fit_decreasing_volume`](volume-sorted.md#first_fit_decreasing_volume)

This gives Auto coverage across EP anchors, guillotine slabs, layer
decomposition, and a fast FFD baseline.

## Tier 2

When `ThreeDOptions.multistart_runs > 0`, Auto also runs:

7. [`multi_start`](meta-strategies.md#multi_start)
8. [`local_search`](meta-strategies.md#local_search)

The default `multistart_runs` is `12`, so tier 2 is enabled by default.
Setting `multistart_runs = 0` disables these meta-strategies and keeps Auto to
the six tier-1 constructions.

## Ranking

Auto keeps the best non-erroring candidate by the 3D solution comparator:

```text
(unplaced.len(), bin_count, total_waste_volume, total_cost)
```

If two candidates tie exactly, the earlier one in the ensemble order remains
selected. The returned solution's `algorithm` field preserves the winning
leaf algorithm name (never the literal `"auto"`). `metrics.notes` receives a
diagnostic note of the form `"auto: tried N algorithms, best was <name>"`.

## Options used by Auto

- `multistart_runs`: controls whether tier 2 runs and how many restarts
  MultiStart/LocalSearch perform.
- `improvement_rounds`: forwarded to LocalSearch.
- `beam_width`: forwarded to the Guillotine 3D candidate.
- `seed`: forwarded to randomized tier-2 candidates.

`auto_exact_max_types` and `auto_exact_max_quantity` are not currently
consumed by the 3D Auto implementation.

`guillotine_required`: when `true`, Auto switches to a dedicated
guillotine-only ensemble (see below). `branch_and_bound` must be selected
explicitly.

## Guillotine-required ensemble (`guillotine_required = true`)

When `ThreeDOptions.guillotine_required = true`, Auto replaces both tiers with
a dedicated guillotine-only sweep over eight candidates:

1. [`guillotine_3d`](guillotine.md#guillotine_3d)
2. [`guillotine_3d_best_short_side_fit`](guillotine.md#guillotine_3d_best_short_side_fit)
3. [`guillotine_3d_best_long_side_fit`](guillotine.md#guillotine_3d_best_long_side_fit)
4. [`guillotine_3d_shorter_leftover_axis`](guillotine.md#guillotine_3d_shorter_leftover_axis)
5. [`guillotine_3d_longer_leftover_axis`](guillotine.md#guillotine_3d_longer_leftover_axis)
6. [`guillotine_3d_min_volume_split`](guillotine.md#guillotine_3d_min_volume_split)
7. [`guillotine_3d_max_volume_split`](guillotine.md#guillotine_3d_max_volume_split)
8. [`layer_building_guillotine`](layer-building.md#layer_building_guillotine)

Only candidates that return `ThreeDSolution.guillotine = true` are considered.
The EP, MultiStart, LocalSearch, and non-guillotine layer variants are skipped
entirely. This path always returns a guillotine-compatible solution or an error
if none of the candidates succeed.

## Complexity

Auto's cost is the sum of its candidates. With default options it runs eight
candidate strategies; with `multistart_runs = 0`, it runs six. For
latency-sensitive callers that have already profiled their workload, selecting
a single algorithm directly is cheaper and more predictable.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDOptions, solve_3d};

// Default: Auto is selected for you.
let solution = solve_3d(problem, ThreeDOptions::default())?;

// Guillotine-required mode:
let solution = solve_3d(
    problem,
    ThreeDOptions {
        guillotine_required: true,
        ..Default::default()
    },
)?;
assert!(solution.guillotine);
```

Source: [`crates/bin-packing/src/three_d/auto.rs`](../../crates/bin-packing/src/three_d/auto.rs).
