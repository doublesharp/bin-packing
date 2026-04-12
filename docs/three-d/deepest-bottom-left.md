# Deepest-bottom-left family

The deepest-bottom-left family implements the event-point placement approach
associated with Karabulut and Inceoglu. Each bin stores event points as
`(z, y, x)` tuples, so iteration naturally tries deepest positions first,
then bottom-most, then left-most.

Both variants sort items by decreasing volume and use the same multi-bin
opening policy. They differ only in how much of the event-point set is scanned
for a single item.

## Event points

Every fresh bin starts with the origin `(0, 0, 0)`. After placing a box at
`(x, y, z)` with extents `(w, h, d)`, the solver inserts up to three
unprojected event points:

- `(x + w, y, z)`
- `(x, y + h, z)`
- `(x, y, z + d)`

Points outside the bin are discarded. v1 does not project event points
backward onto neighbouring placements. This keeps the implementation simple
and deterministic, but can leave more dead space than the projected Extreme
Points family.

## `deepest_bottom_left`

DBL tries only the lexicographically first event point in each open bin. If
the item does not fit there under any allowed rotation, the solver gives up on
that bin for this item and tries the next open bin or a newly opened bin.

This is fast and predictable, but it can miss fill opportunities when the
first event point is blocked and a later point would fit.

## `deepest_bottom_left_fill`

DBLF scans every event point in lexicographic order and places the item at the
first feasible point. This "fill" behavior lets earlier gaps be used before
opening a fresh bin.

DBLF is usually preferable unless the strict DBL behavior is needed for
comparison with literature or external implementations.

## Rotation choice

At a candidate event point, the solver picks the first fitting allowed
rotation in `Rotation3D` declaration order. The implementation computes volume
as the formal primary key, but all rotations of one rectangular box have the
same volume, so declaration order is the effective tiebreaker.

## Complexity

DBL is roughly:

```text
O(n * open_bins * orientations)
```

DBLF adds a scan over event points:

```text
O(n * open_bins * event_points * orientations)
```

Both variants validate overlap by scanning existing placements in the target
bin.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::DeepestBottomLeftFill,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/three_d/dblf.rs`](../../crates/bin-packing/src/three_d/dblf.rs).
