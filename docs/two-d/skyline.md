# Skyline family

The Skyline family represents each sheet by its **top horizon** — a
monotone-in-`x` sequence of `(x, y, width)` segments where each segment's
`y` is the current height of the packed material along that `x` span. A
new item is placed on a skyline segment by moving its footprint down to
the max y of every segment it would cover, then inserting a new horizontal
level at the item's lower edge.

Reference: Jukka Jylänki, *A Thousand Ways to Pack the Bin* (2010),
section on skyline packing. The implementation follows the `SKYLINE-BL`
and `SKYLINE-MW` variants described there.

Both variants share the same skyline representation and update logic;
they differ **only in the comparator** used to pick among fitting
placements.

## Skyline representation

A sheet's skyline is `Vec<SkylineNode>` where `SkylineNode { x, y, width
}` describes one horizontal segment. Invariants:

- **Monotone in x.** Segments are stored in increasing x order; each
  segment's right edge equals the next segment's left edge. There are
  no gaps and no overlaps.
- **A fresh sheet starts with one segment** covering the full sheet
  width at `y = 0`.

When an item is placed, `add_skyline_level`:

1. **Inserts a new node** at the chosen index with `y = item_y +
   item_height` and `width = item_width`.
2. **Walks forward** dropping every subsequent node whose right edge
   lies within the new level's horizontal extent. A node that is only
   partially overlapped is shrunk by advancing its `x` and reducing
   its `width`.
3. **Merges adjacent nodes** with equal `y` via a compact two-pointer
   pass (`merge_nodes`). This keeps the segment count bounded and
   prevents fragmentation from accumulating over many placements.

The insert-at-index-0 edge case (placing the first item on a fresh
sheet) is covered by a regression test in `skyline.rs` that verifies
the monotone-x invariant holds after the update.

## Common mechanism (both variants)

1. **Expand and sort items.** Items are sorted by area descending, with
   `u64` widening to avoid overflow at `MAX_DIMENSION = 1 << 30`.
2. **For each item in that order:**
   1. **Enumerate candidates.** For every (existing sheet, skyline
      node, orientation) triple where the oriented `(width, height)`
      fits starting at that node's x, compute the baseline y where
      the item would rest and the trapped waste under it (see
      *Placement geometry* below). Build a `Candidate`. Also
      enumerate fresh-sheet candidates across every eligible sheet
      type and orientation.
   2. **Pick the best** via the active variant's comparator.
   3. **Place and update** the skyline via `add_skyline_level`.
3. Items with no candidates (nothing fits and no new sheet can be
   opened) are appended to `unplaced`.

## Placement geometry (`skyline_fit`)

Placing an item at skyline node `i`:

1. **Find the baseline y.** Walk nodes starting at `i`, accumulating
   coverage until the item's full width is covered, taking
   `y = max(y, node.y)` at each step. This is where the item rests.
   If the item's lower edge (`y + height`) would exceed the sheet's
   bottom boundary (`sheet.height`), the candidate is rejected.
2. **Compute the trapped waste.** For every node the item's footprint
   spans, the "gap under the item" is `y - node.y`, and the area of
   that gap for this node is `covered_width × gap`. Summed over every
   spanned node, this is the area that becomes permanently trapped
   under the placement. This is what the `MinWaste` variant uses as
   its primary scoring key.

The two passes are structurally identical — both walk nodes forward
from `i` — but the second pass needs `y` from the first, so they
cannot be merged.

## Variants

### `skyline` — Skyline Bottom-Left

**Comparator:** lex `(top, left, waste)`, minimizing, where
`top = y + height` and `left = x`.

Classic bottom-left skyline heuristic adapted to the crate's top-left
coordinate system. Minimizes the **lower edge y** of the placed item first,
then the left edge x, then trapped waste as a tiebreaker. Produces stable
row-like layouts similar to `max_rects_bottom_left` but with a much cheaper
placement step because the skyline representation is one-dimensional along
the x axis.

**Use when** you want a fast, deterministic layout for tall items or
workloads where origin-first row construction matches cutting order or visual
layout needs.

### `skyline_min_waste` — Skyline Minimum Waste

**Comparator:** lex `(waste, top, left)`, minimizing.

Minimizes the **trapped waste under the placed item** first, with
top and left as tiebreakers. On a fresh sheet with a flat skyline at
y=0 there is no trapped waste (the item sits flush), so the new-sheet
score collapses to `(0, height, 0)` and both variants agree. Once the
skyline becomes uneven, MinWaste begins actively preferring
placements that don't leave large gaps under them.

Produces tighter layouts than plain `skyline` on workloads with many
item sizes, at the cost of deeper candidate enumeration (the waste
calculation walks every covered segment). On near-uniform workloads
the two variants typically produce identical output.

**Use when** the workload has mixed item heights and you want to
suppress trapped gaps under the skyline that would otherwise waste
material.

## New-sheet selection

Both variants use a specialized new-sheet comparator that exploits
the flat-skyline invariant: a fresh sheet has one segment at `y=0`,
so placing an item at `(0, 0)` has `waste = 0`, `top = height`,
`left = 0`. The comparator collapses to picking the candidate with
the smallest `top`, then the smallest `cost`, then the smallest
sheet dimensions — which is a strict subset of the full comparator
and is provably correct for both `BottomLeft` and `MinWaste`.

## Complexity

- **Per item:** O(sheets × nodes × orientations) candidate
  enumeration, where each `skyline_fit` walks up to every node in
  the skyline.
- **Per placement:** `add_skyline_level` is O(nodes_overlapped +
  merge_cost), amortized O(1) per item in typical workloads because
  `merge_nodes` collapses transient fragmentation.
- **Total:** O(n² × average_nodes), workload-dependent. Much faster
  than MaxRects in practice because the skyline representation has
  fewer segments than MaxRects has free rects.

All area / volume calculations widen to `u64` before multiplication
to avoid overflow at `MAX_DIMENSION = 1 << 30`.

## Output

- `TwoDSolution.algorithm` is the variant name (`"skyline"` or
  `"skyline_min_waste"`).
- `guillotine = false` — skyline layouts are not guaranteed
  guillotine-compatible (a placement that spans multiple skyline
  nodes at different heights can leave non-guillotine residuals).
- `metrics.iterations = 1`, `metrics.explored_states = 0`.
- `notes` contains `"bottom-left skyline best-fit heuristic"` or
  `"minimum-waste skyline heuristic"`.

## When to use the Skyline family

- When you want **speed over packing quality** relative to MaxRects.
  Skyline is typically the fastest 2D family for large workloads
  because the skyline representation is one-dimensional.
- When items are **sorted by height** in the natural decreasing
  order — skyline does particularly well when tall items are placed
  first and short items fill in under the resulting unevenness.
- When you want a **stable visual layout** with origin-first row construction
  for UI rendering.

## When to avoid it

- When the workload has **many small items** that would otherwise be
  scattered into free-rect gaps. MaxRects typically beats skyline on
  these because it retains multiple disjoint free regions while
  skyline can only see the current horizon.
- When you need **guillotine cuts**. Use the
  [Guillotine family](guillotine.md).

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::SkylineMinWaste,
        ..Default::default()
    },
)?;
```

Both variants are valid:

- `TwoDAlgorithm::Skyline` → `skyline`
- `TwoDAlgorithm::SkylineMinWaste` → `skyline_min_waste`

Source: [`crates/bin-packing/src/two_d/skyline.rs`](../../crates/bin-packing/src/two_d/skyline.rs)
(see `solve_skyline`, `solve_skyline_min_waste`, `SkylineStrategy::compare`,
`skyline_fit`, `add_skyline_level`, `merge_nodes`).
