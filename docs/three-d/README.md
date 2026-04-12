# Three-dimensional rectangular bin packing

The 3D solver packs axis-aligned boxes into axis-aligned bins, minimizing
unplaced items first, then bin count, waste volume, and cost. Every algorithm
accepts multiple bin types, optional per-bin inventory caps, and per-demand
rotation masks over the six axis permutations of `(width, height, depth)`.
The entry point is `solve_3d` and the algorithm is chosen via
`ThreeDOptions.algorithm`.

## Algorithm families

The library exposes 29 selectable 3D algorithm names. Most are grouped into
families that share a placement engine and differ only by scoring, split, or
outer-loop policy.

| Family | Page | Variants | Deterministic |
| --- | --- | --- | --- |
| **Extreme Points** | [extreme-points.md](extreme-points.md) | 6 scoring variants | Yes |
| **Guillotine 3D beam search** | [guillotine.md](guillotine.md) | 7 ranking/split variants | Yes |
| **Layer building** | [layer-building.md](layer-building.md) | 5 inner 2D backends | Yes |
| **Wall and column builders** | [wall-and-column.md](wall-and-column.md) | Wall, column | Yes |
| **Deepest-bottom-left** | [deepest-bottom-left.md](deepest-bottom-left.md) | DBL, DBLF | Yes |
| **Volume-sorted heuristics** | [volume-sorted.md](volume-sorted.md) | FFD volume, BFD volume | Yes |
| **Meta-strategies** | [meta-strategies.md](meta-strategies.md) | MultiStart, GRASP, LocalSearch | Seeded |
| **Branch and bound** | [branch-and-bound.md](branch-and-bound.md) | Restricted exact backend | Yes |
| **Auto** | [auto.md](auto.md) | Tiered ensemble dispatch | Partly seeded |

## Problem shape

```rust
pub struct Bin3D {
    pub name: String,
    pub width: u32,              // x axis, 1..=MAX_DIMENSION_3D
    pub height: u32,             // y axis, vertical, 1..=MAX_DIMENSION_3D
    pub depth: u32,              // z axis, 1..=MAX_DIMENSION_3D
    pub cost: f64,               // per-bin cost, default 1.0
    pub quantity: Option<usize>, // optional inventory cap
}

pub struct BoxDemand3D {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub quantity: usize,
    pub allowed_rotations: RotationMask3D, // default RotationMask3D::ALL
}
```

Dimensions are validated to be strictly positive and at most
`MAX_DIMENSION_3D = 1 << 15`. The smaller 3D cap is deliberate:
`MAX_DIMENSION_3D^3 = 2^45`, leaving headroom in `u64` for accumulating
per-bin and per-run volumes.

## Coordinate system

- Origin is the near-bottom-left corner of the bin.
- `x` grows right, `y` grows upward, and `z` grows toward the front.
- A `Placement3D` stores the placed box's near-bottom-left corner and its
  post-rotation `width`, `height`, and `depth`.
- Edge-touching boxes are valid. Overlap checks use half-open intervals, so
  a box ending at `x = 5` may touch a box beginning at `x = 5`.

## Rotation masks

`Rotation3D` represents the six distinct axis permutations of a rectangular
box:

| Rotation | Placed extents |
| --- | --- |
| `Xyz` | `(w, h, d)` |
| `Xzy` | `(w, d, h)` |
| `Yxz` | `(h, w, d)` |
| `Yzx` | `(h, d, w)` |
| `Zxy` | `(d, w, h)` |
| `Zyx` | `(d, h, w)` |

`RotationMask3D::ALL` enables all six. `RotationMask3D::UPRIGHT` enables
only `Xyz` and `Zyx`, preserving the declared height on the y axis. Duplicate
orientation extents are deduplicated per item, so cubes do not produce six
identical candidates.

## Multi-bin selection

When a construction algorithm needs a fresh bin, it follows a common policy:

1. Filter out bin types whose `quantity` cap is already exhausted.
2. Filter to bin types that can contain the current item, stack, wall, or
   layer in the orientation used by that algorithm.
3. Prefer the smallest compatible bin volume.
4. Break equal-volume ties by `problem.bins` declaration order.

Already-open bins are normally scanned in open order. The BFD volume
heuristic and some 2D-backed builders add their own family-specific
selection policy, documented on their pages.

## Solution ranking

3D solutions are compared lexicographically on:

```text
(unplaced.len(), bin_count, total_waste_volume, total_cost)
```

- **`unplaced.len()`**: placing more boxes always wins.
- **`bin_count`**: fewer consumed bins is the primary optimization target.
- **`total_waste_volume`**: sum of unused volume across every consumed bin.
- **`total_cost`**: sum of `Bin3D.cost` for consumed bins, compared with
  `f64::total_cmp`.

`ThreeDSolution.exact` is reported by `branch_and_bound`, but the current
3D comparator does not use it as a tie-breaker.

## Guillotine compatibility

`ThreeDSolution.guillotine` is set to `true` by every `guillotine_3d*`
variant and by `layer_building_guillotine`. Other 3D algorithms return
`false`, even when a particular output happens to be guillotine-compatible.

`ThreeDOptions.guillotine_required`: when `true`, `ThreeDAlgorithm::Auto`
switches to a guillotine-only ensemble (all seven `guillotine_3d*` variants
plus `layer_building_guillotine`) and returns only candidates where
`solution.guillotine = true`. Selecting a single guillotine variant explicitly
is also valid and avoids the multi-candidate overhead.

## Options

`ThreeDOptions` fields used by current algorithms:

- `algorithm`: chosen `ThreeDAlgorithm`; defaults to `Auto`.
- `beam_width`: live beam size for the Guillotine 3D family; default `8`.
- `multistart_runs`: restarts for `multi_start`, `grasp`, and
  `local_search`; also gates Auto's tier-2 meta-strategy sweep.
- `improvement_rounds`: local-search passes per restart.
- `branch_and_bound_node_limit`: node budget for `branch_and_bound`.
- `seed`: reproducible seed for randomized meta-strategies.

The `auto_exact_max_types` and `auto_exact_max_quantity` fields control when
`Auto` mode may escalate to the exact branch-and-bound backend.

## Reproducibility

Extreme Points, Guillotine, Layer, Wall, Column, DBL/DBLF, FFD/BFD volume,
and Branch-and-Bound are deterministic in the input order. MultiStart, GRASP,
LocalSearch, and Auto's tier-2 meta-strategies use `ThreeDOptions.seed`; when
`seed = None`, they fall back to a fixed internal seed so identical inputs
remain reproducible.

Source: [`crates/bin-packing/src/three_d/model.rs`](../../crates/bin-packing/src/three_d/model.rs)
and [`crates/bin-packing/src/three_d/mod.rs`](../../crates/bin-packing/src/three_d/mod.rs).
