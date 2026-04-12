# MaxRects family

The MaxRects family maintains an explicit set of **free rectangles** — the
maximal axis-aligned empty regions on the sheet. Each placement picks one
free rectangle, pins the item to its top-left corner, and then splits
every free rect the placed item intersects into up to four child rects
(the regions to the left, right, above, and below the placed item). After
splitting, `prune_contained_rects` removes any free rect that is strictly
contained in another, keeping the invariant "every free rect is maximal".

Reference: Jukka Jylänki, *A Thousand Ways to Pack the Bin — A Practical
Approach to Two-Dimensional Rectangle Bin Packing* (2010), the foundational
survey that defined the MaxRects approach and the scoring variants.

All five MaxRects variants share the same free-rect representation, split
rule, and pruning. They differ **only in the comparator** used to rank
candidate placements. Pick the variant that best matches your workload's
geometry.

## Common mechanism (all five variants)

1. **Expand and sort items.** `problem.expanded_items()` produces one
   `ItemInstance2D` per unit of `quantity`. `sort_items_descending`
   orders them by area descending, with a longer-side tiebreaker, and
   widens to `u64` before multiplying to avoid overflow at the
   `1 << 30` dimension cap.
2. **For each item in that order:**
   1. **Enumerate candidates.** For every (existing sheet, free rect,
      orientation) triple where the oriented `(width, height)` fits the
      free rect, build a `PlacementCandidate` pinned to `(free.x,
      free.y)`. For every (new sheet type, orientation) pair where
      inventory is available and the sheet can fit the item, build a
      `NewSheetCandidate` at `(0, 0)` of a fresh sheet.
   2. **Score each candidate** by computing six metrics up front
      (`area_waste`, `short_side_fit`, `long_side_fit`, `bottom`,
      `left`, `contact_score`). Each variant's comparator picks a
      different primary key; the others serve as tiebreakers.
   3. **Pick the best** candidate via `min_by`. Ties among existing-rect
      candidates break by `(sheet_index, free_index)`. Ties among new-
      sheet candidates break by `(cost, stock_index)`.
   4. **Place and split.** The item's `Placement2D` is pushed onto the
      chosen sheet's `placements` list. Every free rect that intersects
      the placed rectangle is split into up to four maximal child rects
      (left of used, right of used, above used, below used — the
      four-way Guillotine-of-free-rects, not to be confused with
      guillotine-compatible layouts). The old intersecting rect is
      discarded.
   5. **Prune.** `prune_contained_rects` removes any free rect that is
      strictly contained in another, with an index tiebreaker so exact
      duplicates collapse to one entry. This is what keeps the
      "maximal" in MaxRects.
3. Items that have no candidates (no free rect fits and no new sheet can
   be opened) are appended to `unplaced`.

**Output.** `TwoDSolution.algorithm` is the variant name,
`guillotine = false`, `metrics.iterations = 1`,
`metrics.explored_states = 0`, `notes` contains a short variant-specific
string.

## Scoring metrics (shared definitions)

| Metric | Definition | Semantic |
| --- | --- | --- |
| `area_waste` | `free_rect.area - used.area` (widened to `u64`) | Area of the free rect that won't be covered by the placed item. Smaller is tighter. |
| `short_side_fit` | `min(free.width - used.width, free.height - used.height)` | Size of the smaller of the two leftover strips along the free rect's axes. Smaller is tighter. |
| `long_side_fit` | `max(free.width - used.width, free.height - used.height)` | Size of the larger leftover strip. |
| `bottom` | `y + used.height` | The lower-edge y of the placed item after placement. |
| `left` | `x` | The left-edge x of the placed item. |
| `contact_score` | See *Contact score* below | Total length of shared edges between the placed item and the sheet walls + already-placed items. Larger is more contact. |

### Contact score

`contact_score` is computed by `contact_score()`:

- **Sheet walls.** Add `height` to the score if the item touches the
  left or right sheet wall (`x == 0` or
  `x + width == sheet.width`). Add `width` if it touches the top or
  bottom wall.
- **Placed items.** For every existing placement, check whether the
  item's left or right edge is flush with the placement's right or
  left edge. If so, add `overlap_len` of the vertical overlap between
  them. Same for top/bottom edges, with horizontal overlap.

The `overlap_len(start_a, end_a, start_b, end_b)` helper is the
standard half-open interval overlap: `end_a.min(end_b).saturating_sub(
start_a.max(start_b))`. Intervals that touch at a single point (e.g.,
`[0, 5)` and `[5, 10)`) contribute zero.

**Rationale.** Higher contact score tends to produce tightly-packed
layouts that "hug" the existing geometry. Unlike area-fit criteria, it
biases toward placements that don't strand long thin gaps between
placed items.

## Complexity

Per placement: O(sheets × free_rects × orientations) candidate
enumeration plus O(free_rects²) pruning. Over `n` items the total is
roughly O(n² × average free rect count), which is workload-dependent
but typically keeps up for `n` on the order of several thousand.

All area calculations widen to `u64` before multiplying to avoid
overflow; `saturating_*` is used for coordinate arithmetic near the
dimension cap.

## Variants

### `max_rects` — Best-Area-Fit (BAF)

**Comparator:** lex `(area_waste, short_side_fit, long_side_fit)`,
minimizing.

The **classic variant** and the MaxRects workhorse. Minimizes the
leftover free-rect area the placed item doesn't cover, which
correlates directly with the amount of free space the current
placement "wastes" relative to the rect it lands in. Area-fit is a
good general-purpose default and tends to produce balanced layouts
on mixed-size workloads.

**Use when** you want a strong general-purpose MaxRects variant
without knowing much about the workload geometry. This is the variant
that `max_rects` (with no qualifier) maps to.

### `max_rects_best_short_side_fit` — Best-Short-Side-Fit (BSSF)

**Comparator:** lex `(short_side_fit, long_side_fit, area_waste)`,
minimizing.

Minimizes the **smaller** of the two leftover edges after placement.
Produces very tight fits along one axis at a time — if the short
leftover is 0, the item lines up perfectly with the free rect on
that axis. BSSF is often the best variant on workloads where items
have one dominant axis (long thin rails, tall cabinets) because the
short leftover is exactly the slack on the non-dominant axis.

**Use when** items are predominantly rectangular-and-elongated and you
want tight alignment along their short axis.

### `max_rects_best_long_side_fit` — Best-Long-Side-Fit (BLSF)

**Comparator:** lex `(long_side_fit, short_side_fit, area_waste)`,
minimizing.

Minimizes the **larger** of the two leftover edges. BLSF biases toward
placements that consume most of the larger dimension of the free rect,
which often helps on workloads with many items close in size — it
prevents a long strip of free rect from being broken up into multiple
near-useless thin residuals.

**Use when** the workload is dominated by items of similar sizes and
you want to keep residuals chunky rather than splintered.

### `max_rects_bottom_left` — Bottom-Left (BL)

**Comparator:** lex `(bottom, left, area_waste)`, minimizing.

Minimizes the lower-edge y of the placed item first, then the left-edge
x, then area waste as a tiebreaker. This is the **classic bottom-left
heuristic** applied to the MaxRects free-rect representation —
implemented in the crate's top-left coordinate system. Placements with a
smaller lower edge win, and among ties, placements closer to the left win.

Produces visibly ordered layouts anchored near the origin. Often weaker than
area-fit for minimizing sheet count but useful when you want deterministic
visualizations or when downstream tooling assumes a simple origin-first
placement order.

**Use when** you need a visually conventional origin-first layout or are
comparing against external literature that assumes the BL heuristic.

### `max_rects_contact_point` — Contact Point

**Comparator:** lex `(−contact_score, area_waste, short_side_fit)`,
minimizing (so higher contact wins).

**Maximizes** total perimeter contact with sheet walls and already-
placed items. The first clause inverts direction (`right.contact_score
.cmp(&left.contact_score)`) so that a higher score sorts lower in the
`min_by`. Tiebreakers are area waste and short-side fit, identical to
BAF/BSSF.

Contact-point MaxRects tends to produce very dense, tightly-packed
layouts that don't strand isolated rectangles in the middle of the
sheet. On difficult workloads with irregular-sized items it often
beats both BAF and BSSF by 1–2 sheets, at the cost of more cache
churn (the scoring loop touches every existing placement, not just
the chosen free rect).

**Use when** the workload is irregular and you want the tightest
packing the MaxRects family can produce, or when you want to minimize
stranded internal gaps.

## When to use the MaxRects family

- As the **default 2D heuristic** — MaxRects is the strongest
  general-purpose family for 2D rectangle packing outside of
  full-strength optimization methods. [Auto](auto.md) runs four of
  the five MaxRects variants by default.
- When you **don't need guillotine cuts**. MaxRects layouts are
  generally not guillotine-compatible because a placed item's split
  leaves up to four child rects that can't be collapsed to a single
  orthogonal cut line.
- When you want **robust output across varied workloads** — the five
  variants cover elongated, balanced, square, and irregular shapes,
  and running Auto exposes the best of them.

## When to avoid it

- When you **need guillotine cuts** for real saw tooling. Use the
  [Guillotine family](guillotine.md) instead.
- When the workload is **uniform** (all items the same size) —
  [Shelf heuristics](shelf.md) will be much faster and produce
  identical-or-better output.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::MaxRectsContactPoint,
        ..Default::default()
    },
)?;
```

All five algorithm names are valid for `TwoDAlgorithm`:

- `TwoDAlgorithm::MaxRects` → `max_rects`
- `TwoDAlgorithm::MaxRectsBestShortSideFit` → `max_rects_best_short_side_fit`
- `TwoDAlgorithm::MaxRectsBestLongSideFit` → `max_rects_best_long_side_fit`
- `TwoDAlgorithm::MaxRectsBottomLeft` → `max_rects_bottom_left`
- `TwoDAlgorithm::MaxRectsContactPoint` → `max_rects_contact_point`

Source: [`crates/bin-packing/src/two_d/maxrects.rs`](../../crates/bin-packing/src/two_d/maxrects.rs)
(see `solve_maxrects`, `solve_maxrects_bssf`, `solve_maxrects_blsf`,
`solve_maxrects_bottom_left`, `solve_maxrects_contact_point`,
`MaxRectsStrategy::compare`, `split_free_rect`, `prune_contained_rects`,
`contact_score`).
