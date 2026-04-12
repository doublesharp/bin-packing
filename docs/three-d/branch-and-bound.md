# Branch and bound

`branch_and_bound` is the restricted exact backend for 3D bin packing. It is
based on the Martello, Pisinger, and Vigo style branch-and-bound approach and
is the only 3D algorithm that can set `ThreeDSolution.exact = true` and
populate `lower_bound`.

## Supported input shape

The backend returns `BinPackingError::Unsupported` unless all restrictions
hold:

- Exactly one bin type: `problem.bins.len() == 1`.
- The bin type is uncapped: `problem.bins[0].quantity.is_none()`.
- Every demand has `allowed_rotations == RotationMask3D::XYZ`.

Use heuristic algorithms for multi-bin-type, capped-inventory, or rotatable
instances.

## Lower bounds

The solver computes:

- **L0**: volume lower bound, `ceil(total_item_volume / bin_volume)`.
- **L1**: fat-item lower bound. Any item whose width, height, and depth are
  all strictly greater than half of the bin's matching extent needs its own
  bin.
- **L2**: `max(L0, L1)`.

`solution.lower_bound` is set to `Some(L2 as f64)`.

## Search

1. Expand items and sort by decreasing volume.
2. Start a depth-first search with no open bins.
3. At each node, take the next largest unplaced item.
4. Branch by trying to place it in each currently open bin, using the Extreme
   Points volume-fit engine to choose the in-bin anchor.
5. Add one more branch that opens a fresh bin for the item.
6. Prune branches whose current bin count cannot beat the incumbent.
7. Stop early if the incumbent bin count equals L2.
8. Stop when `branch_and_bound_node_limit` expanded nodes has been reached.

If the search reaches the node limit before proving optimality, it returns
the best incumbent with `exact = false`. If no incumbent exists yet, a greedy
fallback produces a valid non-exact solution.

## Metrics

- `metrics.branch_and_bound_nodes`: number of expanded B&B nodes.
- `metrics.explored_states`: same node count.
- `metrics.notes`: includes L0, L1, L2, expanded nodes, and whether the limit
  was reached.

## When to use it

Use `branch_and_bound` for small, single-bin-type, no-rotation instances
where a proof matters more than runtime. For general production packing, use
[Auto](auto.md), [Extreme Points](extreme-points.md), or
[Guillotine 3D](guillotine.md).

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::BranchAndBound,
        branch_and_bound_node_limit: 2_000_000,
        ..Default::default()
    },
)?;

if solution.exact {
    assert!(solution.lower_bound.is_some());
}
```

Source: [`crates/bin-packing/src/three_d/exact.rs`](../../crates/bin-packing/src/three_d/exact.rs).
