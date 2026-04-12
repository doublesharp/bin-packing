# Meta-strategies

The 3D meta-strategies wrap deterministic construction engines with multiple
starts, randomized orderings, or local improvement. They all rank candidates
with `ThreeDSolution::is_better_than`.

## `multi_start`

MultiStart runs the Extreme Points volume-fit-residual engine on multiple
random permutations of the expanded item list.

Mechanism:

1. Expand demands into item instances.
2. Seed `SmallRng` from `ThreeDOptions.seed.unwrap_or(0)`.
3. Run `multistart_runs.max(1)` restarts.
4. Shuffle the item list for each restart.
5. Feed the shuffled order directly to the EP volume-fit engine.
6. Keep the best successful solution; transient restart failures become
   `metrics.notes`.

MultiStart is deterministic for a fixed `(problem, options, seed)` triple. It
is included in Auto tier 2 when `multistart_runs > 0`.

## `local_search`

LocalSearch seeds from `first_fit_decreasing_volume`, then explores local
neighbourhood moves over the placed items.

Neighbourhoods:

- **Move**: move a placed item to a feasible extreme point in a different
  open bin.
- **Rotate**: try alternate allowed rotations at the same anchor in the
  current bin.
- **Swap**: exchange two items across different bins if both destinations
  remain feasible.
- **Bin elimination**: target the least-used bin and try to relocate all of
  its items into other bins.

Acceptance is first-improving. Once a move strictly improves the current best
solution, it is committed and the pass restarts.

`multistart_runs.max(1)` controls outer restarts. `improvement_rounds`
controls the maximum number of improvement passes per restart. LocalSearch is
included in Auto tier 2 when `multistart_runs > 0`.

## `grasp`

GRASP combines randomized greedy construction with local search improvement.

Construction uses a restricted candidate list (RCL):

1. Rank remaining items by volume.
2. Let `max_volume` be the largest remaining volume.
3. Put every item with volume at least `0.7 * max_volume` into the RCL.
4. Pick one RCL item uniformly at random.
5. Repeat until all items have an ordering.
6. Run the EP volume-fit engine on that ordering.
7. Apply the same local-search `improve` helper used by `local_search`.

`multistart_runs.max(1)` controls the number of GRASP iterations. The
implementation uses a fixed alpha of `0.3` for the RCL threshold.

## Error behavior

MultiStart and GRASP only return an error if every restart or iteration fails.
Mixed success and failure is represented as notes on the winning solution.
LocalSearch propagates errors from its initial FFD seed.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::Grasp,
        multistart_runs: 24,
        improvement_rounds: 32,
        seed: Some(42),
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/three_d/multi_start.rs`](../../crates/bin-packing/src/three_d/multi_start.rs),
[`crates/bin-packing/src/three_d/grasp.rs`](../../crates/bin-packing/src/three_d/grasp.rs),
and [`crates/bin-packing/src/three_d/local_search.rs`](../../crates/bin-packing/src/three_d/local_search.rs).
