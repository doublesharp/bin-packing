# Two-dimensional rectangular packing

The 2D solver packs axis-aligned rectangles onto axis-aligned sheets,
minimizing the number of sheets consumed and the total wasted area. Every
algorithm honors **per-item rotation control** (`RectDemand2D.can_rotate`),
**multiple sheet types** with independent cost and optional `quantity`
caps. Auto also honors the **guillotine-required** flag, which restricts its
candidate set to orthogonal-cut-compatible layouts. The entry point is
`solve_2d` and the algorithm is chosen via `TwoDOptions.algorithm`.

## Algorithm families

The library ships four construction families plus three meta-strategies.
Each family lives in its own page; the individual variants within each
family appear as H2 sections on the family page.

| Family | Page | Variants | Guillotine? |
| --- | --- | --- | --- |
| **MaxRects** | [max-rects.md](max-rects.md) | 5 scoring variants | No |
| **Skyline** | [skyline.md](skyline.md) | 2 scoring variants | No |
| **Guillotine beam search** | [guillotine.md](guillotine.md) | 7 ranking × split variants | Yes |
| **Shelf heuristics** | [shelf.md](shelf.md) | NFDH, FFDH, BFDH | Layout yes; flag false |
| **Multistart MaxRects** | [multi-start.md](multi-start.md) | 1 randomized meta-strategy | No |
| **Rotation Search** | [rotation-search.md](rotation-search.md) | 1 rotation-assignment meta-strategy | No |
| **Auto** | [auto.md](auto.md) | Ensemble dispatch | Optional |

**Twenty individually selectable algorithm names** cover the cross-product
of family × variant. See each family page for the full list.

## Problem shape

```rust
pub struct Sheet2D {
    pub name: String,
    pub width: u32,              // 1..=MAX_DIMENSION (1 << 30)
    pub height: u32,             // 1..=MAX_DIMENSION
    pub cost: f64,               // per-unit cost (default 1.0)
    pub quantity: Option<usize>, // optional inventory cap
    pub kerf: u32,               // saw-blade kerf width (default 0)
    pub edge_kerf_relief: bool,  // allow trailing placement to overrun by kerf (default false)
}

pub struct RectDemand2D {
    pub name: String,
    pub width: u32,              // 1..=MAX_DIMENSION
    pub height: u32,              // 1..=MAX_DIMENSION
    pub quantity: usize,
    pub can_rotate: bool,        // default true
}
```

Dimensions are validated to be strictly positive and at most
`MAX_DIMENSION = 1 << 30`, chosen so that `MAX_DIMENSION² = 2^60` leaves
`2^4` of headroom in `u64` for summing per-sheet areas across many
sheets without overflow.

## Coordinate system

- Origin is the **top-left corner** of the sheet: `x` grows to the right,
  `y` grows downward.
- A placement's `(x, y)` is the **top-left corner** of the placed
  rectangle, matching the public `Placement2D` model.
- `Placement2D.rotated = true` means the rectangle was rotated 90° from
  its declared `(width, height)` — the stored `width` and `height` in
  the placement are **after rotation**, so `placement.width` and
  `placement.height` are always the actual on-sheet extents.

## Rotation handling

A demand with `can_rotate = true` exposes two orientations to every
algorithm: `(width, height, rotated=false)` and
`(height, width, rotated=true)`. Algorithms iterate both orientations
for every candidate placement and keep whichever scores best under the
active comparator. A demand with `width == height` (square) collapses
to one orientation regardless of `can_rotate`.

`ItemInstance2D::orientations` is the single source of truth for
orientation enumeration and is used identically by MaxRects, Skyline,
Guillotine, and Shelf.

## Multi-sheet selection

Every construction family uses the same pattern when it has to open a
fresh sheet:

1. Filter sheet types by inventory cap
   (`quantity.map(|q| used < q).unwrap_or(true)`).
2. Filter by orientation feasibility (`sheet.width >= item.width &&
   sheet.height >= item.height` across every allowed orientation).
3. Rank the surviving candidates under the family-specific scoring
   criterion (see each family's page). Cost and declaration order are common
   tiebreakers, but the exact tuple is family-specific.

## Solution ranking

2D solutions are compared lexicographically on the tuple:

```
(unplaced.len(), sheet_count, total_waste_area, total_cost,
 Reverse(max_usable_drop_area), Reverse(total_sum_sq_usable_drop_areas))
```

- **`unplaced.len()`** — a solution that places more rectangles always
  wins. If any rectangles could not be placed even with rotation and
  every allowed sheet type, `unplaced` is nonempty.
- **`sheet_count`** — fewer sheets wins. This is the primary
  optimization target.
- **`total_waste_area`** — sum over sheets of `sheet.area -
  used_area`, widened to `u64` to prevent overflow on large sheets.
- **`total_cost`** — sum of `Sheet2D.cost` for each consumed sheet,
  compared via `f64::total_cmp` so NaN is handled deterministically.
- **`Reverse(max_usable_drop_area)`** — among candidates tied on all
  four primary keys, prefer the one with the largest single usable drop.
- **`Reverse(total_sum_sq_usable_drop_areas)`** — final tiebreaker;
  prefer the candidate whose waste is concentrated into fewer, larger
  reusable offcuts.

The two consolidation keys only reorder candidates that are already
equal on every primary key; waste and cost always dominate.

Unlike 1D there is no `exact` flag — no 2D algorithm proves optimality.
The `guillotine_required`
flag does **not** participate in ranking. Instead, when it is set,
[Auto](auto.md) narrows its candidate set to guillotine-compatible
algorithms, so every candidate Auto ranks is already guillotine-valid
by construction.

## `guillotine_required` and `TwoDSolution.guillotine`

- `TwoDOptions.guillotine_required = true` restricts Auto to the
  guillotine family. It has no effect when a specific non-guillotine
  algorithm is explicitly selected — the library will still run that
  algorithm and return its result. Callers can inspect
  `solution.guillotine` to confirm the output is guillotine-compatible.
- `TwoDSolution.guillotine` is set to `true` by every Guillotine variant
  and `false` by MaxRects, Skyline, Shelf, and MultiStart variants. Shelf
  layouts are row-wise guillotine-compatible by construction, but the current
  implementation does not mark the solution flag.

See the [Guillotine page](guillotine.md#guillotine-compatibility) for
the exact definition of guillotine-compatibility.

## Kerf

Each `Sheet2D` accepts an optional `kerf: u32` field (default `0`). When
non-zero, the solver enforces a minimum gap of `kerf` units between every pair
of placements that share an edge — that is, between any two rectangles whose
faces would otherwise be adjacent after cutting.

**Factory-edge rule (D3):** the gap between a placement and the sheet boundary
does not count as a kerf gap. Only internal cuts — cuts that separate two
placed rectangles — consume kerf. This matches physical cutting practice: the
factory edge of the raw sheet is not a cut.

The kerf gap is applied by inflating each placed item's footprint (right and
bottom edges) by `kerf` when computing collisions and free space. The published
`Placement2D` coordinates stay at true item dimensions; the gap is invisible in
the coordinates but shows up in area accounting.

Two new fields report the kerf area consumed:

- `SheetLayout2D.kerf_area` — the area on that sheet attributed to kerf gaps
  between adjacent placements. This is a sub-component of `waste_area` (`waste_area`
  semantics are unchanged: `sheet_area - used_area`).
- `TwoDSolution.total_kerf_area` — sum of `kerf_area` across all layouts.

For the full design rationale and edge-case rules (D1–D7), see the spec at
[docs/superpowers/specs/2026-04-14-kerf-aware-2d-design.md](../superpowers/specs/2026-04-14-kerf-aware-2d-design.md).

### `edge_kerf_relief` (default `false`)

When `true`, the trailing placement on a sheet may extend up to one
`kerf` past the sheet's right and bottom edges. Use this when the
cutting tool is allowed to exit the stock mid-cut (typical on table
and panel saws). Individual parts must still fit within the sheet's
declared dimensions — edge relief only relaxes the *cumulative*
bound on placement position, not the per-part size limit. Placements
stored in the solution retain their finished-part dimensions;
consumers may observe `x + width` values slightly greater than
`sheet.width` and should clip for visualization if needed.

## Usable drops

After all rectangles have been placed, the solver computes the set of
maximal free rectangles that remain on each sheet. Those whose shortest
side is at least `TwoDOptions.min_usable_side` are counted as **usable
drops** — offcuts large enough to be worth keeping for future jobs.

Two metrics are computed per layout:

- `SheetLayout2D.largest_usable_drop_area` — area of the single largest
  qualifying free rectangle on that sheet.
- `SheetLayout2D.sum_sq_usable_drop_areas` — sum of squares of the areas
  of all qualifying free rectangles on that sheet. Higher values indicate
  that waste is concentrated into fewer, larger reusable pieces rather
  than scattered across many small fragments.

These are aggregated into the solution:

- `TwoDSolution.max_usable_drop_area` — the MAX of
  `largest_usable_drop_area` across all layouts (not a sum).
- `TwoDSolution.total_sum_sq_usable_drop_areas` — saturating sum of
  `sum_sq_usable_drop_areas` across all layouts.

### Ranking role

The consolidation metrics act as **tiebreakers only**. The full 2D
ranking key is:

```
(unplaced.len(), sheet_count, total_waste_area, total_cost,
 Reverse(max_usable_drop_area), Reverse(total_sum_sq_usable_drop_areas))
```

The two `Reverse`-wrapped consolidation keys sit AFTER `total_waste_area`
and `total_cost`, so a solution with less waste or lower cost always beats
one with better drop metrics. Consolidation only reorders candidates that
are already equal on every primary key.

Setting `min_usable_side = 0` (the default) counts every non-zero drop,
so consolidation scoring is always active. Raising it to a practical
threshold (e.g. the minimum dimension of your smallest future demand)
filters out narrow strips and focuses the tiebreaker on genuinely reusable
offcuts.

## Cut plans

After solving, call `bin_packing::two_d::cut_plan::plan_cuts(&solution, &options)`
to generate an ordered cut sequence for every sheet in the solution. The planner
is a pure post-processor — it reads the finished layout and emits steps without
re-running the solver.

### Guillotine mode (TableSaw / PanelSaw)

For `TableSaw` and `PanelSaw` presets, the planner reconstructs a **cut tree**
from the placement coordinates. A guillotine-compatible layout can be expressed
as a recursive series of full-width or full-height cuts, each partitioning the
remaining region into two sub-rectangles. The planner:

1. Tries every placement boundary as a candidate cut line (vertical or
   horizontal). A candidate is valid if every placement lies entirely on one
   side.
2. Among valid candidates, prefers the axis that continues the previous cut
   (avoiding unnecessary rotations). Defaults to vertical (rip-first) at the
   start.
3. Recurses on both sub-rectangles (depth-first, left/top child first).

The emitted steps are a depth-first traversal of this tree:

- `CutStep2D::Cut { axis, position }` at each internal node.
- `CutStep2D::FenceReset { new_position }` when the fence must move between
  cuts on the same axis.
- `CutStep2D::Rotate` before any cut whose axis differs from the previous cut.

**Important:** the planner reconstructs the cut tree directly from the
placement coordinates — it does not rely on `TwoDSolution.guillotine`. A layout
produced by a non-guillotine algorithm (e.g., MaxRects on a trivial input) may
still be guillotine-cuttable; reconstruction is the authoritative test.

### CNC router mode (CncRouter)

The `CncRouter` preset handles both guillotine and non-guillotine layouts.
Instead of a cut tree, it emits a **tool path** — a per-placement outline
traversal connecting placements via lift/travel/lower moves:

- `CutStep2D::Cut { axis, position }` for each edge of the placement outline.
- `CutStep2D::ToolUp` / `CutStep2D::ToolDown` between placements.
- `CutStep2D::Travel { to_x, to_y }` from the endpoint of one placement to
  the start of the next.

Placement visit order uses a nearest-neighbor TSP approximation (starting from
each placement's top-left corner, taking the best tour) to minimize total travel
distance.

**CNC mode is the fallback for non-guillotine layouts.** If your layout was
produced by MaxRects, Skyline, or any non-guillotine algorithm and you need a
cut plan, use `CncRouter`.

### Error semantics

- `CutPlanError::NonGuillotineNotCuttable { sheet_name }` — returned when
  `TableSaw` or `PanelSaw` is requested but the layout cannot be expressed as
  a valid guillotine cut sequence. Switch to `CncRouter` or re-solve with
  `guillotine_required = true`.
- `CutPlanError::InvalidOptions(String)` — a cost field is negative, `NaN`,
  or infinite.

### Output shape

```rust
pub struct SheetCutPlan2D {
    pub sheet_name: String,
    pub sheet_index_in_solution: usize,
    pub total_cost: f64,
    pub num_cuts: usize,
    pub num_rotations: usize,
    pub num_fence_resets: usize,
    pub num_tool_ups: usize,
    pub travel_distance: u64,
    pub steps: Vec<CutStep2D>,
}

pub struct CutPlanSolution2D {
    pub preset: CutPlanPreset2D,
    pub effective_costs: EffectiveCosts2D,  // resolved cost values after overrides
    pub sheet_plans: Vec<SheetCutPlan2D>,
    pub total_cost: f64,
}
```

For the full design rationale and algorithmic details, see the spec at
[docs/superpowers/specs/2026-04-15-cut-sequencer-design.md](../superpowers/specs/2026-04-15-cut-sequencer-design.md).

## Reproducibility

- MaxRects, Skyline, Guillotine, and Shelf are **fully deterministic in
  the input order** — no randomness, no `seed` dependency.
- [MultiStart](multi-start.md), [Rotation Search](rotation-search.md)
  (sampled mode), and (when routed through them) [Auto](auto.md)
  consult `TwoDOptions.seed`. When `seed = None` they fall back to a
  fixed internal constant so runs stay reproducible across processes.
  Rotation Search in exhaustive mode is deterministic regardless of
  seed.
- Identical `(problem, options)` pairs — with a fixed `seed` where
  applicable — produce bit-identical output.
