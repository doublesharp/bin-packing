# Two-dimensional rectangular packing

The 2D solver packs axis-aligned rectangles onto axis-aligned sheets,
minimizing the number of sheets consumed and the total wasted area. Every
algorithm honors **per-item rotation control** (`RectDemand2D.can_rotate`),
**multiple sheet types** with independent cost and optional `quantity`
caps. Auto also honors the **guillotine-required** flag, which restricts its
candidate set to orthogonal-cut-compatible layouts. The entry point is
`solve_2d` and the algorithm is chosen via `TwoDOptions.algorithm`.

## Algorithm families

The library ships four construction families plus two meta-strategies.
Each family lives in its own page; the individual variants within each
family appear as H2 sections on the family page.

| Family | Page | Variants | Guillotine? |
| --- | --- | --- | --- |
| **MaxRects** | [max-rects.md](max-rects.md) | 5 scoring variants | No |
| **Skyline** | [skyline.md](skyline.md) | 2 scoring variants | No |
| **Guillotine beam search** | [guillotine.md](guillotine.md) | 7 ranking × split variants | Yes |
| **Shelf heuristics** | [shelf.md](shelf.md) | NFDH, FFDH, BFDH | Layout yes; flag false |
| **Multistart MaxRects** | [multi-start.md](multi-start.md) | 1 randomized meta-strategy | No |
| **Auto** | [auto.md](auto.md) | Ensemble dispatch | Optional |

**Nineteen individually selectable algorithm names** cover the cross-product
of family × variant. See each family page for the full list.

## Problem shape

```rust
pub struct Sheet2D {
    pub name: String,
    pub width: u32,              // 1..=MAX_DIMENSION (1 << 30)
    pub height: u32,             // 1..=MAX_DIMENSION
    pub cost: f64,               // per-unit cost (default 1.0)
    pub quantity: Option<usize>, // optional inventory cap
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
(unplaced.len(), sheet_count, total_waste_area, total_cost)
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

Unlike 1D there is no `exact` flag — no 2D algorithm proves optimality,
and the comparator is only four keys deep. The `guillotine_required`
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

## Reproducibility

- MaxRects, Skyline, Guillotine, and Shelf are **fully deterministic in
  the input order** — no randomness, no `seed` dependency.
- [MultiStart](multi-start.md) and (when routed through it)
  [Auto](auto.md) consult `TwoDOptions.seed`. When `seed = None` they
  fall back to a fixed internal constant so runs stay reproducible
  across processes.
- Identical `(problem, options)` pairs — with a fixed `seed` where
  applicable — produce bit-identical output.
