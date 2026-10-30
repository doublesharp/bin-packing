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

The solver evaluates at most nine integer conservative scales per axis. For
axis capacity `C`, scales include identity and `q = floor(C/(k+1)) + 1` for
`k` from one through eight. Item extents and bin capacity are divided by `q`
with integer floor rounding. Each scale preserves feasible interval-chain
capacity, so transformed item volume divided by transformed bin volume is a
valid count lower bound. The maximum over at most `9^3` combinations is reported
as `solution.lower_bound`.

This includes ordinary volume and greater-than-half restrictions and can detect
stronger multi-slot cases. It is a bounded family, not every possible bound.
Work is independent of expanded quantity and linear in source demand types per
scale combination. See [bounds.rs](../../crates/bin-packing/src/three_d/bounds.rs).

## Search

1. Expand items and sort by decreasing volume.
2. Start a depth-first search with no open bins.
3. At each node, take the next largest unplaced item.
4. Branch by trying to place it in each currently open bin, using the Extreme
   Points volume-fit engine to choose the in-bin anchor.
5. Add one more branch that opens a fresh bin for the item.
6. Prune branches whose current bin count cannot beat the incumbent.
7. Stop early if the incumbent bin count equals the conservative-scale lower bound.
8. Stop when `branch_and_bound_node_limit` expanded nodes has been reached.

Only an incumbent matching the lower bound before a recorded limit cutoff
sets `exact = true`. Exhausting the restricted placement search does not prove
count optimality. A node cutoff returns the best incumbent with `exact = false`. If no incumbent exists yet, a greedy
fallback produces a valid non-exact solution.

## Metrics

- `metrics.branch_and_bound_nodes`: number of expanded B&B nodes.
- `metrics.explored_states`: same node count.
- `metrics.notes`: includes `conservative_scale_bound`, expanded nodes and
  whether the node limit was reached.

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

Effective option defaults are recorded in the [generated reference](../reference.md).
