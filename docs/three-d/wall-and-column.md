# Wall and column builders

The wall and column builders are deterministic constructive heuristics that
reduce 3D packing to a sequence of 2D footprint or face-packing problems.
They are useful when the input has strong geometric structure.

## `wall_building`

Wall building follows the Bischoff and Marriott style vertical wall
construction. The bin is sliced into walls perpendicular to the z axis. Each
wall is packed face-on as a 2D problem with dimensions
`(bin.width, bin.height)`, and the wall depth is fixed by the deepest item in
that wall.

### Mechanism

1. For every item, choose the allowed rotation that maximizes z extent. Ties
   break by rotation declaration order.
2. Sort items by decreasing chosen z extent, then volume, then demand index.
3. Open the smallest compatible bin type whose cap is not exhausted.
4. In that bin, start a wall at the current z offset.
5. Pick the deepest remaining item that fits the remaining bin depth. Its
   z extent becomes the wall depth.
6. Send every remaining item that fits the wall depth and face dimensions to
   the 2D `Auto` backend, with 2D rotation disabled.
7. Convert accepted 2D placements back into 3D placements at the wall z
   offset.
8. Advance the z offset by the wall depth and continue until no item fits.

### Known v1 limitation

Every item is placed at the back of its wall (`z = wall_z_offset`). If an item
is shallower than the wall depth, the front gap is left empty. The algorithm
does not yet run a secondary fill pass inside wall depth.

### Use when

- Items naturally form vertical faces or panels.
- Depth classes are meaningful.
- A 2D face-packing reduction is a good approximation of the real problem.

## `column_building`

Column building creates vertical stacks of items that share an exact floor
footprint under some allowed rotation. Stack footprints are then packed onto
the bin floor as a 2D problem over `(bin.width, bin.depth)`.

### Mechanism

1. For every item, choose the flattest allowed orientation, minimizing y
   extent.
2. Sort by floor footprint area descending.
3. Assign an item to an existing stack only if some allowed rotation matches
   that stack's `(x_extent, z_extent)` exactly and the accumulated stack
   height remains within the tallest bin height.
4. Otherwise, open a new stack using the item's flat footprint.
5. Pack stack footprints into bins with the 2D `Auto` backend.
6. Convert each placed stack into one or more `Placement3D` entries growing
   upward along y.

### Known limitation

The exact-footprint match is intentionally strict. On heterogeneous
catalogues, most items become single-item stacks and the algorithm degenerates
to 2D packing of the bin floor with little 3D benefit.

### Use when

- You have pallet-like or carton-like inputs with repeated footprints.
- Stacking identical bases vertically is desired.
- Vertical order matters more than arbitrary 3D interlocking.

## Output

Both variants set `guillotine = false`, report `iterations = 1`, and leave
search metrics at zero. Any unplaced stacks or wall items are returned as
single-quantity `BoxDemand3D` entries in `solution.unplaced`.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let wall = solve_3d(
    problem.clone(),
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::WallBuilding,
        ..Default::default()
    },
)?;

let columns = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::ColumnBuilding,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/three_d/wall.rs`](../../crates/bin-packing/src/three_d/wall.rs)
and [`crates/bin-packing/src/three_d/column.rs`](../../crates/bin-packing/src/three_d/column.rs).
