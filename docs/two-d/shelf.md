# Shelf family (NFDH / FFDH / BFDH)

The shelf family packs items into horizontal "shelves" — rows of fixed
height determined by the tallest item placed on them. Items are sorted by
height descending and placed onto shelves from left to right; when a
shelf fills up, a new shelf is opened below the current stack of shelves. The
resulting layout is always guillotine-compatible: each shelf corresponds
to a horizontal cut across the sheet, and items inside a shelf are
separated by vertical cuts.

All three shelf variants share the same packing mechanics. They differ
**only in which shelf is considered when placing the next item**:
Next-Fit, First-Fit, or Best-Fit. The common "Decreasing Height" prefix
refers to the item-sort order, which is identical across the three.

Reference: Coffman, Garey, Johnson, and Tarjan, *Performance Bounds for
Level-Oriented Two-Dimensional Packing Algorithms* (1980). The original
formal analysis of NFDH and FFDH, followed by the BFDH variant in the
subsequent literature.

## Common mechanism

1. **Expand and sort items** by declared height descending, with area as a
   tiebreaker and width as the third key. Area widens to `u64` before
   multiplying.
2. **For each item in that order:**
   1. **Try an existing shelf.** Consult the variant-specific
      `choose_existing_shelf`. If a shelf accepts the item, place it at
      `(shelf.used_width, shelf.y)` and increment `shelf.used_width` by
      the placed width.
   2. **Try opening a new shelf** on one of the already-open sheets.
      `choose_new_shelf` consults the same variant selector to find a
      target sheet, then computes the new shelf's `y` as the current
      top of the topmost shelf on that sheet. If the new shelf fits
      within the remaining sheet height, place at `(0, y)` and push a
      new `Shelf { y, height: item.height, used_width: item.width }`.
   3. **Open a fresh sheet.** If neither step above worked, select a
      new sheet via `choose_new_sheet`, which picks the sheet type
      with minimum `waste = sheet.area - item.area` (ties broken by
      cost). The new sheet starts with a single shelf at `y = 0`
      containing the item.
   4. Items that can't fit any sheet in any orientation are appended
      to `unplaced`.
3. The returned `TwoDSolution.metrics.explored_states` tracks one
   increment per item processed. `metrics.iterations = 1`.

**All three variants produce row-wise guillotine-compatible layouts**, but
the current implementation reports `TwoDSolution.guillotine = false` for
Shelf results. Auto's `guillotine_required` path therefore uses the
Guillotine family rather than Shelf.

## Shelf acceptance

A shelf accepts an item in some orientation iff:

- The item's height (in that orientation) is ≤ the shelf's height
  (shelves keep their original height — a shorter item does not
  shrink the shelf, which is the whole reason the DFH order matters).
- The item's width fits in the remaining shelf width: `shelf.used_width
  + item.width ≤ sheet.width`.

When multiple orientations satisfy both checks, the orientation
producing the smaller `remaining_width` after placement wins, with
height as a tiebreaker.

## Variants

### `next_fit_decreasing_height` — NFDH

**Existing-shelf selector:** only the **most recently opened shelf on
the most recently opened sheet** is a candidate. Once a new shelf is
opened, the previous shelf is closed forever — no item is ever placed
on it again, even if it had room.

**New-shelf selector:** only the most recently opened sheet is
eligible to receive a new shelf. When that sheet runs out of vertical
space, NFDH opens a fresh sheet and the current-shelf pointer moves
again.

**Rationale.** NFDH is the simplest shelf strategy: it only tracks
one open shelf at a time, giving it O(1) placement and the tightest
implementation-level memory footprint. Its worst-case approximation
ratio is `2 · OPT + 1` for the number of sheets used.

**Output algorithm name:** `"next_fit_decreasing_height"`.

### `first_fit_decreasing_height` — FFDH

**Existing-shelf selector:** scan every open shelf on every open sheet
in creation order. Place the item on the **first** shelf that accepts
it.

**New-shelf selector:** if no existing shelf accepts the item, scan
open sheets and pick the first one that has enough remaining vertical
space to host a new shelf at the item's height.

**Rationale.** FFDH can reuse earlier shelves that NFDH would have
abandoned, producing strictly-better-or-equal packings at the cost of
per-placement scanning over the open shelves. The worst-case
approximation ratio is `1.7 · OPT + 1` — a tangible improvement over
NFDH.

**Output algorithm name:** `"first_fit_decreasing_height"`.

### `best_fit_decreasing_height` — BFDH

**Existing-shelf selector:** collect every shelf across every sheet
that accepts the item in some orientation, then pick the one with
**smallest `remaining_width`** after placement. Ties break on shelf
height ascending, then sheet / shelf index.

**New-shelf selector:** among open sheets eligible for a new shelf,
pick the one that would leave the smallest `remaining_width` after
the new shelf is placed.

**Rationale.** BFDH closes shelves faster than FFDH because every
placement commits to the tightest available fit. Often beats FFDH by
a small but consistent margin on mixed-width workloads. BFDH is the
shelf variant that [Auto](auto.md) includes in its default ensemble
(NFDH and FFDH are reachable only via explicit selection).

**Note on "best fit".** "Best fit" here means "tightest along the
shelf's fill axis" — the comparator minimizes leftover shelf width
(horizontal slack), not leftover shelf height. Leftover shelf height
is always zero immediately after a shelf is opened (the shelf's
height equals its first item's height) and is fixed for the rest of
the shelf's lifetime, so it does not participate in the comparator.

**Output algorithm name:** `"best_fit_decreasing_height"`.

## New-sheet selection

Shelf variants use a simpler new-sheet comparator than MaxRects /
Skyline / Guillotine: candidates are ranked by
lex `(waste, cost, stock_index)` where
`waste = sheet.area - item.area` widened to `u64`. There is no
scoring-family interaction — once the existing-shelf and new-shelf
paths have been exhausted, the next sheet is simply the tightest-fit
unused sheet by area.

## Complexity

- **NFDH:** O(n) total. The selector is O(1) per item.
- **FFDH:** O(n × total_shelf_count) total. Typically fine because
  the total shelf count grows slowly relative to n.
- **BFDH:** O(n × total_shelf_count) total. Same order as FFDH — the
  best-fit pass touches every candidate shelf — but with a constant
  factor for the `min_by` rather than an early-return scan.
- **Space:** O(n + total_shelf_count). Much smaller than MaxRects or
  Guillotine because shelves are a compact one-dimensional
  representation.

## Output

- `TwoDSolution.algorithm` = `"next_fit_decreasing_height"`,
  `"first_fit_decreasing_height"`, or `"best_fit_decreasing_height"`.
- `guillotine = false` in the current implementation, even though the shelf
  structure itself is guillotine-compatible.
- `metrics.iterations = 1`, `metrics.explored_states = n`.
- `notes = ["decreasing-height shelf packing heuristic"]`.

## When to use the shelf family

- When you need **speed on large workloads**. Shelf variants are the
  fastest 2D family in the library — O(n) to O(n × shelves) with
  tiny constants.
- When items are **strongly sorted by height** — NFDH and FFDH shine
  when the first few items define a tall shelf and subsequent items
  fit comfortably under the resulting row.
- When **guillotine compatibility is required** and you want simpler
  output than the Guillotine beam search. Shelf layouts are trivially
  guillotine by construction: one horizontal cut per shelf, then one
  vertical cut per item within the shelf. If callers need the returned
  `guillotine` flag to be true, select the Guillotine family instead.

## When to avoid it

- When **item heights are highly variable**. A single tall item at
  the start of a shelf leaves unused vertical space around every subsequent
  shorter item in that shelf. MaxRects and Guillotine beam search will pack
  significantly denser on non-uniform-height workloads.
- When you need **the tightest possible packing** and are willing to
  trade time. The shelf family has strictly weaker approximation
  guarantees than MaxRects and the Guillotine family.

## Rust entry point

```rust
use bin_packing::two_d::{TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d};

let solution = solve_2d(
    problem,
    TwoDOptions {
        algorithm: TwoDAlgorithm::BestFitDecreasingHeight,
        ..Default::default()
    },
)?;
assert!(!solution.guillotine);
```

All three variants:

- `TwoDAlgorithm::NextFitDecreasingHeight` → `next_fit_decreasing_height`
- `TwoDAlgorithm::FirstFitDecreasingHeight` → `first_fit_decreasing_height`
- `TwoDAlgorithm::BestFitDecreasingHeight` → `best_fit_decreasing_height`

Source: [`crates/bin-packing/src/two_d/shelf.rs`](../../crates/bin-packing/src/two_d/shelf.rs)
(see `solve_nfdh`, `solve_ffdh`, `solve_bfdh`, `ShelfStrategy`,
`choose_existing_shelf`, `choose_new_shelf`, `compare_existing_candidates`).
