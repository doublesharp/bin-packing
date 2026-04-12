# Guillotine beam search family

The Guillotine family produces layouts that can be realized by a sequence
of edge-to-edge orthogonal cuts through the sheet — the cut pattern that
real panel saws and sheet-cutting machinery can physically execute. Every
variant runs a **beam search** over the item sequence, keeping the best
`beam_width` partial states at each step, and every solution has
`TwoDSolution.guillotine = true`.

Reference: Jukka Jylänki, *A Thousand Ways to Pack the Bin* (2010), the
chapter on Guillotine packing. The split heuristic names (BSSF, BLSF,
SLAS, LLAS, MINAS, MAXAS) follow Jylänki's convention directly.

## Guillotine compatibility

A 2D layout is **guillotine-compatible** if it can be produced by
recursively cutting the sheet with edge-to-edge orthogonal cuts. Every
placement in a guillotine layout must land inside a rectangular region
that can be further subdivided by a single horizontal or vertical cut
into two children, and each child is either the placed item itself or
another guillotine-compatible sub-region.

This construction naturally maps to a **binary split tree** over the
sheet's free area. After each placement, the containing free rectangle
is split into two children by either a horizontal or a vertical cut —
never the four-way MaxRects split. The Guillotine family enforces this
by choosing a split axis per placement; the output is guaranteed
guillotine-compatible by construction.

This is what downstream CAM / saw-control software needs if you are
actually cutting material. A MaxRects or Skyline layout may pack more
items onto the sheet, but cannot generally be reproduced on a panel saw.

## Common mechanism (all seven variants)

1. **Expand and sort items.** Sort by `max(width, height)` descending
   (longest-side first), with area descending as a tiebreaker. Both
   keys widen to `u64` before multiplication.
2. **Seed the beam** with a single empty state: no sheets, no
   placements, zero waste, zero fragmentation.
3. **For each item in order:**
   1. For every state in the current beam:
      1. **Enumerate candidates** by scanning (existing sheet × free
         rect × orientation) and (new sheet type × orientation) for
         every feasible placement. Each candidate records its
         `waste = free_rect.area - item.area`, `short_side_fit`,
         `long_side_fit`, and `incremental_cost`.
      2. **Apply the split heuristic** to produce one or two candidates
         per (free rect × orientation) tuple, depending on the heuristic
         (see *Split heuristics* below).
      3. **Rank candidates** by the active variant's comparator and
         truncate to the **top 6** per state. This is the beam's
         per-state branching factor.
   2. For each surviving candidate: **clone the state**, place the
      item, execute the split, and push the child state onto the
      next beam.
   3. If a state has no candidates at all (the item doesn't fit
      anywhere and no new sheet can be opened), clone the state with
      the item appended to `unplaced`.
4. **Prune the next beam** to `beam_width.max(1)` by sorting under the
   beam-state comparator (which ranks on `unplaced`, sheet count,
   waste, cost, then fragmentation — the full 2D solution ranking plus a
   final fragmentation term).
5. After processing every item, return the best remaining beam state.

**Output fields:** `TwoDSolution.algorithm` = variant name,
`guillotine = true`, `metrics.iterations = n` (one per item processed),
`metrics.explored_states` = total beam states considered,
`notes` contains a short description of the active strategy and split
heuristic.

## Split: horizontal vs. vertical

When an item is placed at the top-left corner of a free rectangle
of size `(fw, fh)`, the residual free area is split into two child
rectangles. The two canonical splits are:

- **Horizontal split** (axis name convention: horizontal == "cut runs
  horizontally"):
  - Lower child: `(free.x, free.y + h, fw, fh - h)` — full-width strip
    below the placement.
  - Right child: `(free.x + w, free.y, fw - w, h)` — short strip to
    the right of the placement, same height as the placement.
- **Vertical split**:
  - Right child: `(free.x + w, free.y, fw - w, fh)` — full-height
    strip to the right of the placement.
  - Lower child: `(free.x, free.y + h, w, fh - h)` — short strip below
    the placement, same width as the placement.

The placement coordinates are identical either way; what differs is
which child strip gets the full extent and which gets the truncated
extent. The choice matters because it determines what shapes the
downstream solver can still place.

## Split heuristics

Five split heuristics are exposed, each producing a different axis
preference per placement:

- **`BeamBoth`** — push **both** the horizontal and vertical split as
  separate candidates into the beam. The search explores both branches
  in parallel, doubling the per-item branching factor but removing the
  need for a heuristic axis choice. Used by `guillotine`,
  `guillotine_best_short_side_fit`, and `guillotine_best_long_side_fit`.
- **`ShorterLeftoverAxis` (SLAS)** — pick the axis whose leftover
  strip is *shorter*. If `free.width - used.width <= free.height -
  used.height`, use vertical (preserves the wider residual); otherwise
  use horizontal. Biases toward keeping the larger usable strip
  intact.
- **`LongerLeftoverAxis` (LLAS)** — the inverse of SLAS. Picks the
  axis whose leftover strip is *longer*, preserving the smaller
  residual.
- **`MinAreaSplit`** — pick the axis that minimizes the maximum-area
  child, i.e., chooses the split that produces a smaller "biggest"
  residual. Useful when you want the free space to be broken up into
  more-balanced pieces.
- **`MaxAreaSplit`** — the inverse. Picks the axis that maximizes the
  maximum-area child, preserving one large residual and one small
  one.

All area comparisons in the split heuristic widen to `u64` before
multiplication.

## Ranking strategies

Three candidate ranking criteria score candidates before they enter
the beam. All three use `min_by`, so smaller is better.

- **`BestAreaFit` (BAF)** — lex `(waste, short_side_fit,
  long_side_fit)`. Minimize the free-rect area the placement doesn't
  cover.
- **`BestShortSideFit` (BSSF)** — lex `(short_side_fit,
  long_side_fit, waste)`. Minimize the smaller leftover edge.
- **`BestLongSideFit` (BLSF)** — lex `(long_side_fit,
  short_side_fit, waste)`. Minimize the larger leftover edge.

These are the same scoring metrics as the [MaxRects family](max-rects.md),
with one difference: Guillotine does not track `bottom`/`left` or
`contact_score`. Those metrics would require a free-rect topology
MaxRects maintains but Guillotine does not (Guillotine's split tree
keeps only the two resulting children, not the overall maximal free
region).

## Variants

The library exposes **seven variants** covering a useful cross-section
of the ranking × split matrix. The remaining combinations are not
exposed to keep the algorithm enumeration tractable and because the
exposed subset has been shown to bracket the useful performance range
on typical workloads.

### `guillotine` — Best-Area-Fit + BeamBoth

The **default guillotine variant**. Ranks candidates by area waste and
explores both split axes in the beam. Good general-purpose starting
point when you know you need guillotine cuts but don't have a specific
workload shape in mind.

### `guillotine_best_short_side_fit` — BSSF + BeamBoth

Ranks by shorter leftover edge, with both splits explored in the beam.
Typically tighter than BAF on elongated items where you want alignment
along the short axis.

### `guillotine_best_long_side_fit` — BLSF + BeamBoth

Ranks by longer leftover edge, with both splits explored. Tighter than
BAF on workloads with many similar-size items where preserving the
larger leftover edge keeps the free area chunky.

### `guillotine_shorter_leftover_axis` — BAF + SLAS

BAF ranking with a forced SLAS split. Preserves the wider residual
strip after each placement, which tends to produce fewer but larger
free rectangles. Favors workloads where you want to keep maximum
flexibility for future large items.

### `guillotine_longer_leftover_axis` — BAF + LLAS

BAF ranking with a forced LLAS split. Preserves the narrower residual
strip. Favors workloads where you want to "use up" the large strip
immediately and reserve the small strips for small items.

### `guillotine_min_area_split` — BAF + MinArea

BAF ranking with a forced minimum-max-child split. Chooses the split
axis that produces the smaller of the two maximum-area children,
biasing toward balanced residuals. Good for workloads with an even
size distribution.

### `guillotine_max_area_split` — BAF + MaxArea

BAF ranking with a forced maximum-max-child split. Chooses the split
axis that leaves one large residual and one small one. Good for
workloads with very mixed sizes where you want to reserve large
rectangles for later large items.

## Beam width

`TwoDOptions.beam_width` (default `8`) controls the beam size. At
`beam_width = 1` the algorithm degrades to a pure greedy construction.
At `beam_width = ∞` (not recommended) it becomes exhaustive over the
top-6 per-state branching, which is still a small fraction of the full
search tree.

Higher beam widths typically improve packing density by 1–3% at the
cost of linear time and memory. A value of 8 is a good default for
problems up to several hundred items; 16 or 32 makes sense if you are
willing to trade seconds for a tighter result.

## Complexity

- **Per item:** O(beam_width × sheets × free_rects × orientations)
  candidate enumeration, followed by a sort and truncate to the top
  6. The beam is then re-sorted and truncated to `beam_width`.
- **Total:** O(n × beam_width × expansion_per_state). In practice the
  per-state expansion is tightly bounded because the top-6 truncation
  and the beam width cap the branching factor.
- **Space:** Each beam state is a deep clone of the current layout
  (sheets, placements, free rects). On a tight beam the total memory
  is dominated by the beam width × current item count.

## Output

- `TwoDSolution.algorithm` = one of the seven variant names.
- `guillotine = true`.
- `metrics.iterations = n` (one per input item).
- `metrics.explored_states` = running total of beam states expanded.
- `notes` contains the ranking and split-heuristic description.

## When to use the Guillotine family

- When you need **actual guillotine cuts** for panel saws or other
  edge-to-edge cutting machinery. Every MaxRects and Skyline layout
  has to be post-processed to check guillotine compatibility;
  Guillotine gives you the guarantee for free.
- When you want **tighter packing than the Shelf family** while still
  staying guillotine-compatible. Guillotine beam search typically
  beats NFDH/FFDH/BFDH on non-trivial workloads.
- When the **[Auto](auto.md) gate** sets `guillotine_required = true`.
  Auto narrows its candidate set to guillotine variants in that case.

## When to avoid it

- When **guillotine compatibility is not required** — the MaxRects
  family will typically produce denser layouts because it doesn't
  have to respect the recursive split invariant.
- When the workload is **huge** and you need raw speed.
  [Shelf heuristics](shelf.md) are much faster and still produce
  guillotine-compatible layouts at the cost of density.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::GuillotineBestShortSideFit,
        beam_width: 16,
        ..Default::default()
    },
)?;
assert!(solution.guillotine);
```

All seven algorithm names:

- `TwoDAlgorithm::Guillotine` → `guillotine`
- `TwoDAlgorithm::GuillotineBestShortSideFit` → `guillotine_best_short_side_fit`
- `TwoDAlgorithm::GuillotineBestLongSideFit` → `guillotine_best_long_side_fit`
- `TwoDAlgorithm::GuillotineShorterLeftoverAxis` → `guillotine_shorter_leftover_axis`
- `TwoDAlgorithm::GuillotineLongerLeftoverAxis` → `guillotine_longer_leftover_axis`
- `TwoDAlgorithm::GuillotineMinAreaSplit` → `guillotine_min_area_split`
- `TwoDAlgorithm::GuillotineMaxAreaSplit` → `guillotine_max_area_split`

Source: [`crates/bin-packing/src/two_d/guillotine.rs`](../../crates/bin-packing/src/two_d/guillotine.rs)
(see `solve_guillotine`, `solve_guillotine_bssf`, `solve_guillotine_blsf`,
`solve_guillotine_slas`, `solve_guillotine_llas`,
`solve_guillotine_min_area_split`, `solve_guillotine_max_area_split`,
`GuillotineStrategy::compare`, `push_split_candidates`,
`preferred_split_axis`).
