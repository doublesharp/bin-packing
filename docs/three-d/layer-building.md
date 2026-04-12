# Layer-building family

Layer building decomposes a bin into horizontal layers stacked along the y
axis. The first item assigned to a layer fixes that layer's height. Items in
the same layer sit on the layer floor and are packed as a 2D footprint
problem over `(bin.width, bin.depth)`.

The family follows George and Robinson style layer construction and delegates
each layer's footprint packing to the shared 2D `place_into_sheet` engine.

## Common mechanism

1. Expand demands into item instances.
2. For each item, choose the flattest allowed orientation, meaning the
   rotation with minimum y extent. Ties break by `Rotation3D` declaration
   order.
3. Sort items by decreasing flat y extent, with footprint area as a
   secondary key.
4. Open the smallest compatible bin type whose cap is not exhausted.
5. Start a layer at the current y offset. The first pending item fixes the
   layer height.
6. Collect every pending item whose flat y extent fits under that layer
   height.
7. Pack the candidates into a synthetic 2D sheet of size
   `(bin.width, bin.depth)`.
8. Convert 2D placements back into `Placement3D` at the current layer y
   offset.
9. Advance the layer y offset by the layer height and repeat until the bin's
   height is exhausted.

## Known v1 limitation

Layer height is fixed by the first item in the layer. Shorter items sit on
the layer floor and the vertical slab above them is not infilled by later
layers. Workloads with highly mixed y extents can waste substantial volume.
For those cases, try [Extreme Points](extreme-points.md),
[`deepest_bottom_left_fill`](deepest-bottom-left.md), or the
[volume-sorted heuristics](volume-sorted.md).

## Variants

### `layer_building`

Uses the 2D `Auto` backend for each layer. This is the broadest layer-building
variant and is included in 3D Auto tier 1.

### `layer_building_max_rects`

Uses the 2D `max_rects` backend inside each layer. This is a strong default
when guillotine compatibility is not required.

### `layer_building_skyline`

Uses the 2D `skyline` backend inside each layer. This can be faster than
MaxRects on workloads that behave like height-map packing across the layer
floor.

### `layer_building_guillotine`

Uses the 2D `guillotine` backend inside each layer and sets
`ThreeDSolution.guillotine = true`. Use this when every layer must be
guillotine-compatible.

### `layer_building_shelf`

Uses the 2D `best_fit_decreasing_height` shelf backend inside each layer.
This is the fastest and simplest layer backend, but usually less dense than
MaxRects or Guillotine.

## Complexity

Layer building performs repeated 2D solves over subsets of the pending item
list. The outer loop is roughly O(number_of_layers * pending_items), plus the
cost of the selected 2D backend for each layer.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::LayerBuildingGuillotine,
        ..Default::default()
    },
)?;
assert!(solution.guillotine);
```

Source: [`crates/bin-packing/src/three_d/layer.rs`](../../crates/bin-packing/src/three_d/layer.rs).
