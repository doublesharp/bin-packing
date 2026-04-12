# Guillotine 3D beam-search family

The Guillotine 3D family produces layouts represented by recursive
three-slab partitions. Placing a box into a free cuboid creates up to three
new free cuboids:

- **Right slab**: space to the positive x side of the placed box.
- **Top slab**: space above the placed box.
- **Front slab**: space to the positive z side of the placed box.

Empty slabs are discarded. Every returned solution from this family has
`ThreeDSolution.guillotine = true`.

## Common mechanism

All seven variants share one beam-search engine:

1. Expand demands and sort items by decreasing volume.
2. Start with one empty beam node: no open bins, no placements, all items
   remaining.
3. At each step, take the first remaining item in each live beam node.
4. Enumerate every feasible placement into existing free cuboids and allowed
   rotations.
5. If no open bin accepts the item, open the smallest compatible bin type and
   enumerate placements at its root free cuboid.
6. Apply the placement, replace the chosen free cuboid with right/top/front
   slabs, and accumulate primary and secondary scores.
7. Sort child nodes by `(primary_score, secondary_score)` and keep the best
   `beam_width.max(1)` nodes.
8. When leaves are reached, build solutions and return the best one under the
   3D comparator.

If an item cannot fit any open or newly opened bin in a branch, that branch
records the item as unplaced and continues with the remaining items.

## Ranking rules

Primary ranking is lower-is-better:

| Ranking | Definition |
| --- | --- |
| `VolumeFit` | `free_cuboid.volume - item.volume` |
| `ShortSide` | `min(leftover_w, leftover_h, leftover_d)` |
| `LongSide` | `max(leftover_w, leftover_h, leftover_d)` |

The first three variants differ by this primary ranking and use no secondary
split score.

## Split scores

The physical partition is always the same three-slab split. The "split"
variants use a secondary score to prefer one residual shape over another:

| Split score | Definition |
| --- | --- |
| `Default` | `0` |
| `ShorterLeftoverAxis` | `min(leftover_w, leftover_h, leftover_d)` |
| `LongerLeftoverAxis` | `max(leftover_w, leftover_h, leftover_d)` |
| `MinVolumeSplit` | Minimum volume among the three new slabs |
| `MaxVolumeSplit` | `u64::MAX - max(slab_volume)` |

`MaxVolumeSplit` is inverted because the beam comparator minimizes.

## Variants

### `guillotine_3d`

Best-volume-fit ranking with the default secondary score. This is the
general-purpose guillotine baseline and is included in Auto tier 1.

### `guillotine_3d_best_short_side_fit`

Ranks by the smallest leftover edge. It favors placements that become flush
or nearly flush on at least one axis.

### `guillotine_3d_best_long_side_fit`

Ranks by the largest leftover edge. It favors placements that reduce the
largest residual dimension.

### `guillotine_3d_shorter_leftover_axis`

Uses volume-fit primary ranking and the shorter-leftover secondary score.
Useful when you want the beam to prefer placements that leave at least one
small residual edge.

### `guillotine_3d_longer_leftover_axis`

Uses volume-fit primary ranking and the longer-leftover secondary score.
Useful when the residual shape's largest axis matters more than the smallest.

### `guillotine_3d_min_volume_split`

Uses volume-fit primary ranking and prefers smaller minimum slab volume. This
biases toward consuming or eliminating one of the child slabs.

### `guillotine_3d_max_volume_split`

Uses volume-fit primary ranking and prefers preserving one large child slab.
This can help when later large boxes need contiguous free volume.

## Beam width

`ThreeDOptions.beam_width` defaults to `8`. At `beam_width = 1`, the family
acts like a greedy search over the current best child. Larger values explore
more alternatives at roughly linear additional memory and time cost.

## Complexity

Per beam step:

```text
O(beam_width * open_bins * free_cuboids * orientations)
```

Each child node clones its partial state, so memory grows with beam width and
the number of placements retained in live nodes.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::Guillotine3DMaxVolumeSplit,
        beam_width: 16,
        ..Default::default()
    },
)?;
assert!(solution.guillotine);
```

Source: [`crates/bin-packing/src/three_d/guillotine.rs`](../../crates/bin-packing/src/three_d/guillotine.rs).
