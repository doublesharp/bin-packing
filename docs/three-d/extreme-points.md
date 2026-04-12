# Extreme Points family

The Extreme Points family implements the Crainic, Perboli, and Tadei style
3D placement heuristic. Each open bin maintains a set of candidate anchor
points called extreme points. For each item, the solver evaluates every
feasible `(open bin, extreme point, allowed rotation)` triple, places the
item at the best-scoring anchor, then projects new extreme points from the
placed box's far faces.

All six variants share the same engine. They differ only in the scoring rule
used to rank feasible placements.

## Common mechanism

1. Expand demands into one item per unit of `quantity`.
2. Sort items by decreasing volume.
3. For each item, scan existing bins in open order.
4. Inside each bin, evaluate every extreme point and every allowed
   deduplicated rotation.
5. Place the item at the candidate with the lowest variant-specific score.
6. Remove the consumed extreme point and project new points from the placed
   box.
7. If no open bin accepts the item, open the smallest compatible bin type
   whose `quantity` cap is not exhausted.
8. If no bin type can be opened, append the item to `unplaced`.

Candidate ties break by `(y, x, z, rotation_order, demand_index)`, which makes
the output stable across runs.

## Extreme-point generation

The engine stores points in `(z, y, x)` order so the natural tree order is
"deepest, then bottom, then left". After placing a box at `(x, y, z)` with
extents `(w, h, d)`, it removes that point and inserts projected anchors on
the positive x, y, and z faces. Projection keeps anchors on valid support
surfaces and avoids generating obvious overlaps.

`metrics.extreme_points_generated` reports the number of EP anchors generated
by the run. Non-EP algorithms leave this metric at zero.

## Scoring variants

### `extreme_points`

Volume-fit residual scoring. The score is the remaining volume in the target
bin after accounting for already-placed volume and the candidate item volume.
Smaller is tighter.

Use this as the general EP baseline. It is included in Auto tier 1.

### `extreme_points_residual_space`

Residual-space scoring from CPT08. The score is:

```text
min(gap_x, gap_y, gap_z)
```

where each gap is the distance from the placed box's far face to the
corresponding bin wall at the candidate anchor. Smaller values bias toward
flush fits on at least one axis. This variant is included in Auto tier 1.

### `extreme_points_free_volume`

Free-volume scoring from CPT08. The score is:

```text
gap_x * gap_y * gap_z
```

This approximates the residual cuboid anchored after placement. It is useful
when the shape of the remaining corner volume matters more than total bin
volume consumed.

### `extreme_points_bottom_left_back`

Bottom-left-back scoring. The score encodes `(y, x, z)` into a single integer,
so candidates closer to the bottom, then left, then back of the bin win.

This variant produces visually conventional gravity-like layouts. It is often
less dense than fit-based scoring, but useful for comparisons and predictable
placement order.

### `extreme_points_contact_point`

Contact-point scoring. The raw score is the total area of contact with bin
walls and already-placed boxes. Because the shared comparator minimizes,
the implementation stores it as `u64::MAX - raw_contact`.

Neighbour contact is computed as the union of contact rectangles on each face,
not a naive sum per neighbour, so overlapping contact regions are not double
counted. This variant is included in Auto tier 1.

### `extreme_points_euclidean`

Euclidean scoring from CPT08. The score is:

```text
x^2 + y^2 + z^2
```

with each axis widened before multiplication. This favors anchors closest to
the origin and is another stable "pack from the corner" strategy.

## Complexity

For `n` expanded items, the main cost is:

```text
O(n * open_bins * extreme_points_per_bin * orientations)
```

The number of extreme points grows with placements, so dense workloads can
approach quadratic behavior. All volume and contact-area arithmetic widens to
`u64`.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, ThreeDProblem, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::ExtremePointsContactPoint,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/three_d/extreme_points.rs`](../../crates/bin-packing/src/three_d/extreme_points.rs).
