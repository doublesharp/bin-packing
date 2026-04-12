# Volume-sorted FFD and BFD

The volume-sorted heuristics are simple deterministic baselines built on top
of the Extreme Points placement engine. Both expand demand quantities, sort
items by decreasing volume, and use `ExtremePointsScoring::VolumeFitResidual`
inside each selected bin.

## Shared setup

1. Expand demands into one item per unit of `quantity`.
2. Sort items by decreasing `width * height * depth`, widened to `u64`.
3. For each item, try to place it into currently open bins using the Extreme
   Points placement engine.
4. If no open bin accepts it, open the smallest compatible bin type whose
   quantity cap is not exhausted.
5. If no bin type can be opened, append the item to `unplaced`.

## `first_fit_decreasing_volume`

FFD volume scans open bins in open order and commits the first bin that
accepts the item. It is the fastest 3D construction heuristic in the catalog
and is included in Auto tier 1.

Use it as a cheap baseline, for latency-sensitive cases, or as a seed for
other strategies.

## `best_fit_decreasing_volume`

BFD volume evaluates every open bin. For each candidate bin, it snapshots the
bin state, attempts the EP placement, and measures the resulting used volume.
The bin with the largest post-placement used volume wins, which is equivalent
to the smallest leftover volume among accepted bins. Ties keep the earlier
open bin.

BFD usually costs more than FFD but can reduce waste on mixed workloads.

## Relationship to Extreme Points

These heuristics reuse EP placement inside one selected bin. They do not
change the in-bin anchor scoring. The difference is only the policy used to
select which open bin receives the next volume-sorted item.

## Complexity

FFD:

```text
O(n * open_bins_until_first_fit * extreme_points * orientations)
```

BFD:

```text
O(n * open_bins * extreme_points * orientations)
```

BFD also clones EP state for tentative placements, so its constant factor is
higher.

## Rust entry point

```rust
use bin_packing::three_d::{ThreeDAlgorithm, ThreeDOptions, solve_3d};

let solution = solve_3d(
    problem,
    ThreeDOptions {
        algorithm: ThreeDAlgorithm::BestFitDecreasingVolume,
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/three_d/sorted.rs`](../../crates/bin-packing/src/three_d/sorted.rs).
