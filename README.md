<p align="center">
  <img src="https://raw.githubusercontent.com/doublesharp/bin-packing/main/docs/bin-packing.png" alt="bin-packing" width="200">
</p>

# Bin Packing

A Rust-first cut list and bin packing optimization library for one-dimensional
cutting stock (linear bar / pipe stock), two-dimensional rectangular sheet
stock, and three-dimensional box-into-bin packing problems. The crate is
`#![forbid(unsafe_code)]`, integer-math oriented, and `serde`-friendly so
problems and solutions travel cleanly across process boundaries and language
bindings.

- Core crate: [crates/bin-packing](crates/bin-packing/) — published to
  [crates.io](https://crates.io/crates/bin-packing) as `bin-packing`.
- Node.js bindings: [bindings/node](bindings/node/) — published to npm as
  `@0xdoublesharp/bin-packing`, powered by
  [`napi-rs`](https://napi.rs/).
- WebAssembly bindings: [bindings/wasm](bindings/wasm/) — published to npm as
  `@0xdoublesharp/bin-packing-wasm`, powered by
  [`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/). Runs in
  browsers, Node.js, Deno, Bun, and Cloudflare Workers.
- Fuzz targets: [fuzz/](fuzz/) — libFuzzer harnesses for the 1D, 2D, and 3D
  entry points.

## Features

### 1D cutting stock

- **Multi-stock support.** Mix any number of stock dimensions with independent
  `length`, `kerf`, `trim`, `cost`, and optional inventory `available` caps.
- **Kerf and trim modeling.** Each layout deducts trim from usable length and
  charges kerf between adjacent cuts, so results match physical cut lists.
- **Inventory-aware procurement.** When stock has `available` caps, the
  solver returns both the capped solution and a relaxed-inventory estimate
  so you can see per-stock `used_quantity`, `required_quantity`, and
  `additional_quantity_needed`.
- **Multi-objective ranking.** Solutions are compared lexicographically by
  `(unplaced, stock_count, total_waste, total_cost, !exact)` so better
  heuristic candidates win automatically in `Auto` mode.
- **Lower bounds.** The column-generation backend reports an LP lower bound
  and marks the solution `exact = true` when it matches the incumbent.
- **Structured errors.** Public boundary validation fails fast with typed
  `BinPackingError` variants instead of panics.

### 3D box packing

- **Multi-bin support.** Mix any number of bin types with independent
  `width`, `height`, `depth`, `cost`, and optional `quantity` caps.
- **Per-item rotation control.** Each demand has an `allowed_rotations`
  bitmask over the six axis-permutation orientations of `(w, h, d)`.
  `RotationMask3D::ALL` allows all six; `RotationMask3D::UPRIGHT` permits
  only the two orientations that keep the declared `y` axis vertical.
- **29-algorithm catalog.** Includes Extreme Points (6 scoring variants),
  Guillotine 3D beam search (7 variants), horizontal layer-building (5
  inner-2D backends), Bischoff & Marriott vertical wall-building,
  column / stack building, Deepest-Bottom-Left and DBLF, volume-sorted
  FFD/BFD, Multi-start, GRASP, and Local Search meta-strategies, plus a
  restricted Martello-Pisinger-Vigo branch-and-bound exact backend.
- **Same ranking contract.** Solutions are compared lexicographically by
  `(unplaced, bin_count, total_waste_volume, total_cost, !exact)`, so
  `Auto` mode always returns the best candidate.
- **Dimension safety.** Per-axis dimensions are capped at `1 << 15`
  (`MAX_DIMENSION_3D`) so that `w × h × d` never overflows `u64`.

### 2D rectangular packing

- **Multi-sheet support.** Mix any number of sheet types with independent
  `width`, `height`, `cost`, and optional `quantity` caps.
- **Per-item rotation control.** Each demand has its own `can_rotate` flag;
  the solver enumerates the legal orientations per item.
- **Guillotine mode.** `guillotine_required = true` restricts the Auto
  strategy to guillotine-compatible constructions and sets
  `TwoDSolution.guillotine` on the result.
- **Kerf-aware packing.** Each sheet type accepts an optional `kerf` gap (in
  the same units as the sheet dimensions). The solver enforces the gap between
  every pair of adjacent placements (factory edges are free; only internal cuts
  consume kerf). The solution reports `kerf_area` per layout and
  `total_kerf_area` across all sheets.
- **Edge kerf relief.** Set `Sheet2D.edge_kerf_relief = true` to allow a
  trailing placement to extend up to one `kerf` past the sheet boundary,
  modeling a blade that exits the stock. Parts must still fit within the
  declared sheet dimensions.
- **Multistart search.** A randomized MaxRects meta-strategy (`multi_start`)
  permutes input orderings under a reproducible `seed`.
- **Rotation search.** Exhaustive (or sampled) rotation assignment search
  (`rotation_search`) enumerates all 2^k rotation assignments for k
  rotatable demand types, finding globally better rotation choices than
  greedy per-placement rotation.
- **Usable-drop consolidation.** Set `TwoDOptions.min_usable_side` to
  filter out narrow leftover strips that are too small to reuse. The
  solver reports `SheetLayout2D.largest_usable_drop_area` (largest
  maximal free rectangle meeting the threshold on that sheet),
  `SheetLayout2D.sum_sq_usable_drop_areas` (sum of squares of all
  qualifying drops on that sheet), and aggregates across the solution as
  `TwoDSolution.max_usable_drop_area` (the MAX over all layouts, not the
  sum) and `TwoDSolution.total_sum_sq_usable_drop_areas` (saturating sum
  across layouts). Candidates that are equal on waste and cost are
  tiebroken in favour of larger / more consolidated drops; this tiebreaker
  sits AFTER `total_waste_area` and `total_cost` so waste and cost always
  dominate.
- **Integer-safe math.** Dimensions are widened to `u64` before area
  calculations; `u32 * u32` on dimensions is forbidden by the workspace
  lint policy.

### Cut plans

After solving, you can generate an ordered cut plan for any `OneDSolution` or
`TwoDSolution`. The cut planner is a pure post-processor — it reads a finished
layout and emits a per-bar or per-sheet sequence of `CutStep`s optimized for a
chosen shop cost model. The solver is not re-run.

**Entry points:**

- `bin_packing::one_d::cut_plan::plan_cuts(&solution, &options) -> Result<CutPlanSolution1D, CutPlanError>`
- `bin_packing::two_d::cut_plan::plan_cuts(&solution, &options) -> Result<CutPlanSolution2D, CutPlanError>`

**Presets (1D):**

| Preset | `cut_cost` | `fence_reset_cost` |
| --- | --- | --- |
| `ChopSaw` | 1.0 | 0.3 |

**Presets (2D):**

| Preset | `cut_cost` | `rotate_cost` | `fence_reset_cost` | `tool_up_down_cost` | `travel_cost` |
| --- | --- | --- | --- | --- | --- |
| `TableSaw` | 1.0 | 2.0 | 0.5 | — | — |
| `PanelSaw` | 1.0 | 5.0 | 0.3 | — | — |
| `CncRouter` | 1.0 | — | — | 0.2 | 0.01 |

Any cost field on the options struct overrides the preset default. Pass
`..CutPlanOptions2D::default()` to accept all preset defaults.

**Output:**

- `CutPlanSolution1D` — contains `bar_plans: Vec<BarCutPlan1D>` and a
  solution-level `total_cost`. Each `BarCutPlan1D` carries:
  - `steps: Vec<CutStep1D>` — ordered `Cut` and `FenceReset` steps.
  - `total_cost`, `num_cuts`, `num_fence_resets`.
- `CutPlanSolution2D` — contains `sheet_plans: Vec<SheetCutPlan2D>` and a
  solution-level `total_cost`. Each `SheetCutPlan2D` carries:
  - `steps: Vec<CutStep2D>` — ordered `Cut`, `Rotate`, `FenceReset`,
    `ToolUp`, `ToolDown`, and `Travel` steps.
  - `total_cost`, `num_cuts`, `num_rotations`, `num_fence_resets`,
    `num_tool_ups`, `travel_distance`.

Total cost is the linear sum:
`num_cuts * cut_cost + num_rotations * rotate_cost + num_fence_resets * fence_reset_cost + num_tool_ups * tool_up_down_cost + travel_distance * travel_cost`.

**Errors (`CutPlanError`, `#[non_exhaustive]`):**

- `NonGuillotineNotCuttable { sheet_name }` — the layout cannot be expressed
  as a sequence of full guillotine cuts, and the `TableSaw` or `PanelSaw`
  preset was requested. Switch to `CncRouter` or re-solve with
  `guillotine_required = true`.
- `InvalidOptions(String)` — a cost field is negative, `NaN`, or infinite.

**Quick example (Rust):**

```rust
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
    cut_plan::{CutPlanOptions2D, CutPlanPreset2D, plan_cuts},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "plywood".into(),
            width: 96,
            height: 48,
            cost: 1.0,
            quantity: None,
            kerf: 2,
        }],
        demands: vec![
            RectDemand2D { name: "panel-a".into(), width: 24, height: 18, quantity: 4, can_rotate: true },
            RectDemand2D { name: "panel-b".into(), width: 12, height: 12, quantity: 6, can_rotate: true },
        ],
    };

    let solution = solve_2d(problem, TwoDOptions { algorithm: TwoDAlgorithm::Auto, ..Default::default() })?;

    let cut_plan = plan_cuts(
        &solution,
        &CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() },
    )?;

    println!("total cut plan cost: {}", cut_plan.total_cost);
    for sheet_plan in &cut_plan.sheet_plans {
        println!("  {}: {} steps, cost {:.2}", sheet_plan.sheet_name, sheet_plan.steps.len(), sheet_plan.total_cost);
    }

    Ok(())
}
```

### Cross-cutting

- **`serde` round-tripping.** Every request, option, solution, and metrics
  type derives `Serialize` / `Deserialize`.
- **Reproducibility.** All randomized strategies accept a `seed`; runs with
  identical seeds and inputs produce identical output.
- **Metrics.** Each solution carries a `metrics` block
  (`iterations`, `explored_states`, etc.) plus free-form `notes`, so callers
  can surface diagnostics without re-running.
- **Benchmarks.** Criterion benches live in
  [crates/bin-packing/benches/solver_benches.rs](crates/bin-packing/benches/solver_benches.rs).
- **Fuzzing.** `cargo fuzz run solver_inputs` exercises the 1D, 2D, and 3D
  solvers against randomized inputs.

## Algorithms

Selected via `OneDOptions.algorithm`, `TwoDOptions.algorithm`, or
`ThreeDOptions.algorithm`. All names below are the exact `snake_case` strings
accepted by `serde` and the Node / WebAssembly bindings.

### 3D (`ThreeDAlgorithm`)

Extreme Points family — maintains a set of extreme points (EPs) and places
each item at the EP that scores best under the chosen criterion:

| Name | Description |
| ---- | ----------- |
| `extreme_points` | Volume-fit residual scoring (default EP variant). |
| `extreme_points_residual_space` | Crainic-Perboli-Tadei "RS" — scores by residual space remaining after placement. |
| `extreme_points_free_volume` | CPT "FV" — scores by free volume in the remaining space. |
| `extreme_points_bottom_left_back` | Bottom-left-back tiebreaking (place as close to the origin as possible). |
| `extreme_points_contact_point` | Score by surface contact with already-placed items or bin walls. |
| `extreme_points_euclidean` | CPT "EU" — scores by Euclidean distance of the EP from the bin origin. |

Guillotine 3D beam search — recursive three-slab partition with configurable
split and ranking rules:

| Name | Description |
| ---- | ----------- |
| `guillotine_3d` | Beam search with best-volume-fit ranking. |
| `guillotine_3d_best_short_side_fit` | Ranked by the shortest leftover edge after placement. |
| `guillotine_3d_best_long_side_fit` | Ranked by the longest leftover edge after placement. |
| `guillotine_3d_shorter_leftover_axis` | Split along the axis that leaves the shorter leftover slab. |
| `guillotine_3d_longer_leftover_axis` | Split along the axis that leaves the longer leftover slab. |
| `guillotine_3d_min_volume_split` | Split to minimise the volume of the new sub-cuboid. |
| `guillotine_3d_max_volume_split` | Split to maximise the volume of the new sub-cuboid. |

Layer building — packs items into horizontal layers; each layer uses an
independent 2D inner backend:

| Name | Description |
| ---- | ----------- |
| `layer_building` | Auto inner-2D backend (picks best of MaxRects, Skyline, Guillotine). |
| `layer_building_max_rects` | MaxRects inner backend. |
| `layer_building_skyline` | Skyline inner backend. |
| `layer_building_guillotine` | Guillotine inner backend. |
| `layer_building_shelf` | Best-fit-decreasing-height shelf inner backend. |

Geometry-based heuristics:

| Name | Description |
| ---- | ----------- |
| `wall_building` | Bischoff & Marriott 1990 vertical wall-building: builds planar walls of items from floor to ceiling. |
| `column_building` | Vertical stack / column building with 2D footprint packing for column placement. |
| `deepest_bottom_left` | Deepest-Bottom-Left (Karabulut & İnceoğlu): place each item at the deepest, then bottom-most, then leftmost feasible position. |
| `deepest_bottom_left_fill` | Deepest-Bottom-Left-Fill: adds a fill pass to resolve deadlocks in DBL. |
| `first_fit_decreasing_volume` | Sort items by volume descending; assign each to the first bin with space. |
| `best_fit_decreasing_volume` | Sort items by volume descending; assign each to the bin with the least remaining volume that still fits. |

Meta-strategies:

| Name | Description |
| ---- | ----------- |
| `multi_start` | Randomized EP meta-strategy. Runs `multistart_runs` restarts with shuffled item orderings under `seed`. |
| `grasp` | Greedy Randomized Adaptive Search: RCL-based randomized construction (top-30% by volume) followed by local search improvement. |
| `local_search` | Standalone local search seeded from FFD volume. Explores move/rotate/swap neighbourhood with bin-elimination repair. |
| `branch_and_bound` | Restricted Martello-Pisinger-Vigo exact backend. Computes L0/L1/L2 lower bounds; returns `Unsupported` for multi-bin-type, capped, or rotation-constrained inputs. |
| `auto` *(default)* | Tiered sweep: tier-1 runs ExtremePoints (3 variants), Guillotine3D, LayerBuilding, and FFD; tier-2 (when `multistart_runs > 0`) adds MultiStart and LocalSearch. Returns the best result. |

Solution ranking for 3D is lexicographic on
`(unplaced, bin_count, total_waste_volume, total_cost, !exact)`.

### 1D (`OneDAlgorithm`)

| Name | Description |
| ---- | ----------- |
| `auto` *(default)* | Runs FFD, BFD, and local search, then optionally escalates to column generation when the instance is small enough (`auto_exact_max_types`, `auto_exact_max_quantity`, single uncapped stock). Returns the best candidate. |
| `first_fit_decreasing` | Classic FFD: sort cuts by length descending, place each into the first open bin that fits, open a new bin otherwise. O(n²) deterministic. |
| `best_fit_decreasing` | BFD: place each sorted cut into the open bin with the tightest fit. Typically uses fewer bins than FFD on mixed sizes. |
| `local_search` | Multistart local search seeded from FFD/BFD, with bin-elimination repair. `multistart_runs` restarts × `improvement_rounds` swap/move rounds, driven by `seed`. |
| `column_generation` | Exact backend: generates cutting patterns via price-and-solve, refines with pattern search, and enumerates up to `exact_pattern_limit` patterns per iteration for `column_generation_rounds` rounds. Reports an LP lower bound and sets `exact = true` when the bound is matched. |

### 2D (`TwoDAlgorithm`)

MaxRects family — maintains an explicit set of free rectangles, scoring
each candidate placement under a different criterion and then splitting /
merging free rectangles:

| Name | Description |
| ---- | ----------- |
| `max_rects` | Best-area-fit MaxRects (the classic variant). |
| `max_rects_best_short_side_fit` | Prefer placements that minimize the shorter leftover edge. |
| `max_rects_best_long_side_fit` | Prefer placements that minimize the longer leftover edge. |
| `max_rects_bottom_left` | Bottom-left-first tiebreaking. |
| `max_rects_contact_point` | Score by perimeter contact with already-placed items or the sheet boundary. |

Skyline family — maintains a monotone skyline along one axis:

| Name | Description |
| ---- | ----------- |
| `skyline` | Place each item at the lowest feasible skyline segment. |
| `skyline_min_waste` | Skyline construction with waste-minimizing candidate ranking, which keeps sub-skyline gaps smaller. |

Guillotine beam search — recursive two-stage cuts with a beam-width
search front (`beam_width`) and configurable split / ranking rules:

| Name | Description |
| ---- | ----------- |
| `guillotine` | Beam search with default (best-area-fit) ranking and default split. |
| `guillotine_best_short_side_fit` | Beam search ranked by shorter leftover edge. |
| `guillotine_best_long_side_fit` | Beam search ranked by longer leftover edge. |
| `guillotine_shorter_leftover_axis` | Split along the axis that leaves the shorter leftover strip. |
| `guillotine_longer_leftover_axis` | Split along the axis that leaves the longer leftover strip. |
| `guillotine_min_area_split` | Split to minimize the area of the new sub-rectangle. |
| `guillotine_max_area_split` | Split to maximize the area of the new sub-rectangle. |

Shelf heuristics — simple fast baselines that stack items into horizontal
"shelves" sized by the tallest item on each shelf:

| Name | Description |
| ---- | ----------- |
| `next_fit_decreasing_height` | NFDH: open a new shelf as soon as the current one overflows. |
| `first_fit_decreasing_height` | FFDH: place each item on the first shelf that fits. |
| `best_fit_decreasing_height` | BFDH: place each item on the tightest-fitting shelf. |

Meta-strategies:

| Name | Description |
| ---- | ----------- |
| `multi_start` | Randomized MaxRects meta-strategy. Runs `multistart_runs` restarts with permuted item orderings under `seed`. |
| `rotation_search` | Exhaustive (or sampled) rotation assignment search: enumerates all 2^k rotation assignments for k rotatable demand types (or samples `multistart_runs` random assignments when k exceeds `auto_rotation_search_max_types`). Each assignment fixes rotations and packs via MaxRects best-area-fit. |
| `auto` *(default)* | Runs MaxRects (best-area, BSSF, BLSF, contact), Skyline and Skyline-min-waste, BFDH, Guillotine BSSF and shorter-leftover-axis, Multistart, and RotationSearch, then returns the best candidate. When `guillotine_required` is set, only the guillotine variants run. |

Solution ranking for 2D is lexicographic on
`(unplaced, sheet_count, total_waste_area, total_cost, max_usable_drop_area↑, total_sum_sq_usable_drop_areas↑)`.
The two consolidation keys (↑ = prefer larger) only reorder candidates that
are already equal on every primary key; waste and cost always win. When
`guillotine_required` is set, `auto` narrows its candidate set to the
guillotine variants — the guillotine constraint is enforced by candidate
selection, not by a ranking tie-break, so every candidate `auto` ranks is
already guillotine-compatible.

## Rust usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
bin-packing = "0.3"
```

The dimension modules expose three entry points:

- `one_d::solve_1d(problem, options) -> Result<OneDSolution>`
- `two_d::solve_2d(problem, options) -> Result<TwoDSolution>`
- `three_d::solve_3d(problem, options) -> Result<ThreeDSolution>`

All problem and solution types implement `serde::Serialize` /
`serde::Deserialize`, so the same models are usable from Rust code or
wire-format APIs. The crate root re-exports `BinPackingError` and `Result`;
everything else comes from the `one_d`, `two_d`, and `three_d` modules.

### 1D cutting stock

```rust
use bin_packing::one_d::{
    CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, Stock1D, solve_1d,
};

fn main() -> bin_packing::Result<()> {
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "96in bar".into(),
            length: 96,
            kerf: 1,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![
            CutDemand1D { name: "rail".into(), length: 45, quantity: 2 },
            CutDemand1D { name: "brace".into(), length: 30, quantity: 2 },
        ],
    };

    let solution = solve_1d(
        problem,
        OneDOptions {
            algorithm: OneDAlgorithm::Auto,
            ..Default::default()
        },
    )?;

    println!("algorithm: {}", solution.algorithm);
    println!("stock used: {}", solution.stock_count);
    println!("waste: {}", solution.total_waste);
    println!("exact: {}", solution.exact);
    if let Some(bound) = solution.lower_bound {
        println!("lower bound: {bound}");
    }

    for layout in &solution.layouts {
        println!(
            "{}: used {} / {}",
            layout.stock_name, layout.used_length, layout.stock_length
        );
    }

    for requirement in &solution.stock_requirements {
        println!(
            "{}: used {}, required {}, additional {}",
            requirement.stock_name,
            requirement.used_quantity,
            requirement.required_quantity,
            requirement.additional_quantity_needed,
        );
    }

    Ok(())
}
```

`Stock1D` fields:

- `length`: raw stock length before trim is removed.
- `kerf`: material lost to the saw between adjacent cuts.
- `trim`: unusable material removed from the stock length before packing.
- `cost`: per-unit cost of consuming one piece of this stock type.
- `available`: optional inventory cap (number of pieces of this type that
  may be used).

`OneDOptions` fields:

- `algorithm`: `auto` *(default)*, `first_fit_decreasing`,
  `best_fit_decreasing`, `local_search`, or `column_generation`.
- `multistart_runs`: number of restarts for local search (default `16`).
- `improvement_rounds`: improvement passes per local-search start (default
  `24`).
- `column_generation_rounds`: outer rounds for the exact backend (default
  `32`).
- `exact_pattern_limit`: maximum patterns enumerated per round in the exact
  backend (default `25_000`).
- `auto_exact_max_types`: maximum distinct demand types for `Auto` mode to
  attempt the exact backend (default `14`).
- `auto_exact_max_quantity`: maximum total demand quantity for `Auto` mode
  to attempt the exact backend (default `96`).
- `seed`: optional RNG seed for reproducible randomized algorithms.

`OneDSolution` fields of interest:

- `algorithm` / `exact` / `lower_bound`
- `stock_count`, `total_waste`, `total_cost`
- `layouts`: per-piece `StockLayout1D` entries (stock name, used/remaining
  length, waste, cost, and cut list), sorted by utilization descending.
- `stock_requirements`: per-stock procurement summary including any
  shortage against the declared `available` inventory. When inventory caps
  are present this is computed from a relaxed-inventory `Auto` re-solve.
- `unplaced`: cuts the solver could not place (normally empty unless
  inventory capped the solution).
- `metrics`: `iterations`, `generated_patterns`, `enumerated_patterns`,
  `explored_states`, and diagnostic `notes`.

### 2D rectangular packing

```rust
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

fn main() -> bin_packing::Result<()> {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "plywood".into(),
            width: 96,
            height: 48,
            cost: 1.0,
            quantity: None,
            kerf: 2,
        }],
        demands: vec![
            RectDemand2D {
                name: "panel-a".into(),
                width: 24,
                height: 18,
                quantity: 4,
                can_rotate: true,
            },
            RectDemand2D {
                name: "panel-b".into(),
                width: 12,
                height: 12,
                quantity: 6,
                can_rotate: true,
            },
        ],
    };

    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::Auto,
            seed: Some(42),
            ..Default::default()
        },
    )?;

    println!("algorithm: {}", solution.algorithm);
    println!("sheets used: {}", solution.sheet_count);
    println!("waste area: {}", solution.total_waste_area);
    println!("guillotine: {}", solution.guillotine);

    for layout in &solution.layouts {
        println!(
            "{}: {} placements, {} waste area",
            layout.sheet_name,
            layout.placements.len(),
            layout.waste_area
        );
        for placement in &layout.placements {
            println!(
                "  {} @ ({}, {}) {}x{}{}",
                placement.name,
                placement.x,
                placement.y,
                placement.width,
                placement.height,
                if placement.rotated { " rotated" } else { "" },
            );
        }
    }

    Ok(())
}
```

`Sheet2D` fields:

- `width`, `height`: sheet dimensions (positive, up to `1 << 30`).
- `cost`: per-unit cost of consuming one sheet of this type.
- `quantity`: optional cap on the number of sheets of this type that may be
  used.
- `kerf`: optional saw-blade kerf width (default `0`). The solver enforces
  this gap between every pair of adjacent placements on the sheet; the gap
  between a placement and the sheet edge is free (factory edge rule).
- `edge_kerf_relief`: when `true`, the trailing placement on the sheet
  may extend up to one `kerf` past the sheet's right and bottom edges,
  modeling a cut that exits the stock (default `false`). Individual parts
  must still fit within the sheet's declared dimensions.

`RectDemand2D` fields:

- `width`, `height`: rectangle dimensions (positive, up to `1 << 30`).
- `quantity`: number of identical rectangles required.
- `can_rotate`: whether the solver may rotate this rectangle 90°.

`TwoDOptions` fields:

- `algorithm`: see the 2D algorithm table above; defaults to `auto`.
- `multistart_runs`: number of restarts for the multistart meta-strategy.
- `beam_width`: beam width for the guillotine beam search backend.
- `guillotine_required`: when `true`, forces guillotine-compatible layouts
  and restricts `auto` to guillotine variants.
- `min_usable_side`: minimum side length (in the same units as sheet
  dimensions) for a leftover drop to count as usable (default `0` — all
  drops count). Raise this to ignore narrow strips that are too small to
  reuse; the consolidation tiebreakers then favour solutions that leave
  fewer but larger reusable offcuts.
- `auto_rotation_search_max_types`: maximum number of rotatable demand types
  for which rotation search uses exhaustive enumeration (default `16`). When
  the number of rotatable types exceeds this threshold, rotation search
  switches to sampling `multistart_runs` random assignments.
- `seed`: optional RNG seed for reproducible randomized algorithms.

`TwoDSolution` fields of interest:

- `algorithm`, `guillotine`
- `sheet_count`, `total_waste_area`, `total_kerf_area`, `total_cost`
- `max_usable_drop_area`: the largest single usable drop area across all
  layouts (MAX over layouts, not a sum). A drop qualifies when both its
  sides are at least `min_usable_side`.
- `total_sum_sq_usable_drop_areas`: saturating sum of `sum_sq_usable_drop_areas`
  across all layouts. Higher values indicate better drop consolidation
  (fewer but larger reusable offcuts).
- `layouts`: per-sheet `SheetLayout2D` entries (sheet name, dimensions,
  placements, used area, waste area, `kerf_area` — the portion of
  `waste_area` attributable to kerf gaps — `largest_usable_drop_area`,
  and `sum_sq_usable_drop_areas`).
- `placements`: each `Placement2D` carries `x`, `y`, `width`, `height`,
  and `rotated` (whether the rectangle was rotated from its declared
  orientation).
- `unplaced`: demands the solver could not place.
- `metrics`: `iterations`, `explored_states`, and diagnostic `notes`.

### 3D box packing

```rust
use bin_packing::three_d::{
    Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions,
    ThreeDProblem, solve_3d,
};

fn main() -> bin_packing::Result<()> {
    let problem = ThreeDProblem {
        bins: vec![Bin3D {
            name: "pallet".into(),
            width: 120,
            height: 100,
            depth: 80,
            cost: 1.0,
            quantity: None,
        }],
        demands: vec![
            BoxDemand3D {
                name: "carton-a".into(),
                width: 30,
                height: 25,
                depth: 20,
                quantity: 6,
                allowed_rotations: RotationMask3D::ALL,
            },
            BoxDemand3D {
                name: "carton-b".into(),
                width: 20,
                height: 15,
                depth: 10,
                quantity: 8,
                allowed_rotations: RotationMask3D::UPRIGHT,
            },
        ],
    };

    let solution = solve_3d(
        problem,
        ThreeDOptions {
            algorithm: ThreeDAlgorithm::Auto,
            seed: Some(42),
            ..Default::default()
        },
    )?;

    println!("algorithm: {}", solution.algorithm);
    println!("bins used: {}", solution.bin_count);
    println!("waste volume: {}", solution.total_waste_volume);

    for layout in &solution.layouts {
        println!(
            "{}: {} placements, {} waste volume",
            layout.bin_name,
            layout.placements.len(),
            layout.waste_volume
        );
        for placement in &layout.placements {
            println!(
                "  {} @ ({}, {}, {}) {}x{}x{} rotation={:?}",
                placement.name,
                placement.x, placement.y, placement.z,
                placement.width, placement.height, placement.depth,
                placement.rotation,
            );
        }
    }

    Ok(())
}
```

`Bin3D` fields:

- `width`, `height`, `depth`: bin dimensions (positive, up to `1 << 15`).
- `cost`: per-unit cost of consuming one bin of this type.
- `quantity`: optional cap on the number of bins of this type that may be used.

`BoxDemand3D` fields:

- `width`, `height`, `depth`: declared box dimensions (positive, up to `1 << 15`).
- `quantity`: number of identical boxes required.
- `allowed_rotations`: bitmask of the six axis-permutation orientations permitted
  for this box. Use `RotationMask3D::ALL` (all six), `RotationMask3D::UPRIGHT`
  (only `Xyz` and `Zyx`, keeping the y-axis vertical), or construct a custom mask
  from the per-rotation constants (`RotationMask3D::XYZ`, `XZY`, `YXZ`, `YZX`,
  `ZXY`, `ZYX`).

`ThreeDOptions` fields:

- `algorithm`: see the 3D algorithm table above; defaults to `auto`.
- `multistart_runs`: number of restarts for randomized meta-strategies.
- `improvement_rounds`: improvement passes per local-search or GRASP start.
- `beam_width`: beam width for the Guillotine 3D beam search backend.
- `seed`: optional RNG seed for reproducible randomized algorithms.
- `auto_exact_max_types` / `auto_exact_max_quantity`: thresholds controlling
  when `Auto` mode may escalate to the exact branch-and-bound backend.
- `branch_and_bound_node_limit`: maximum nodes the exact backend may expand.

`ThreeDSolution` fields of interest:

- `algorithm`, `exact`, `lower_bound`, `guillotine`
- `bin_count`, `total_waste_volume`, `total_cost`
- `layouts`: per-bin `BinLayout3D` entries (bin name, dimensions, placements,
  `used_volume`, `waste_volume`).
- `placements`: each `Placement3D` carries `x`, `y`, `z`, `width`, `height`,
  `depth`, and `rotation` (the `Rotation3D` variant applied).
- `bin_requirements`: per-bin procurement summary (populated when at least one
  `Bin3D.quantity` cap is set).
- `unplaced`: boxes the solver could not place.
- `metrics`: `iterations`, `explored_states`, `extreme_points_generated`,
  `branch_and_bound_nodes`, and diagnostic `notes`.

### Errors

`BinPackingError` is `#[non_exhaustive]`. Match it with a wildcard arm.

- `InvalidInput(String)` — problem failed boundary validation (empty
  stock/sheet list, zero dimension, non-finite cost, trim ≥ length, etc.).
- `Infeasible1D { item, length }` — a 1D demand does not fit any declared
  stock.
- `Infeasible2D { item, width, height }` — a 2D demand does not fit any
  declared sheet, even with rotation.
- `Infeasible3D { item, width, height, depth }` — a 3D demand does not fit
  any declared bin in any allowed rotation.
- `Unsupported(String)` — the exact backend encountered a configuration it
  does not currently handle (for example, multi-stock input or capped
  inventory for column generation), or an internal invariant violation was
  caught by a defensive check at runtime.

## Node.js usage

Build the binding from [bindings/node](bindings/node/):

```sh
pnpm install
pnpm run build
```

Then call the wrapper with plain JavaScript objects. Field names match the
Rust `serde` representation (`snake_case`):

```js
const binPacking = require('@0xdoublesharp/bin-packing');

const cutList = binPacking.solve1d(
  {
    stock: [{ name: 'bar', length: 100, kerf: 1, available: 5 }],
    demands: [
      { name: 'A', length: 45, quantity: 2 },
      { name: 'B', length: 30, quantity: 2 }
    ]
  },
  { algorithm: 'auto', seed: 7 }
);

console.log(cutList.stock_count);
console.log(cutList.stock_requirements);
console.log(cutList.unplaced);

const layout = binPacking.solve2d(
  {
    sheets: [{ name: 'plywood', width: 96, height: 48, kerf: 2 }],
    demands: [
      { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true }
    ]
  },
  { algorithm: 'auto', guillotine_required: true, beam_width: 12 }
);

console.log(layout.sheet_count, layout.total_waste_area, layout.guillotine);

const packing = binPacking.solve3d(
  {
    bins: [{ name: 'pallet', width: 120, height: 100, depth: 80 }],
    demands: [
      { name: 'carton-a', width: 30, height: 25, depth: 20, quantity: 6 },
      { name: 'carton-b', width: 20, height: 15, depth: 10, quantity: 8 }
    ]
  },
  { algorithm: 'auto', seed: 42 }
);

console.log(packing.bin_count, packing.total_waste_volume);
```

TypeScript type definitions ship alongside the JavaScript wrapper in
[bindings/node/wrapper.d.ts](bindings/node/wrapper.d.ts). Both camelCase
(`solve1D` / `solve2D` / `solve3D`) and lowercase (`solve1d` / `solve2d` /
`solve3d`) aliases are exported, plus a `version()` helper.

## Browser / WASM usage

For browsers, Deno, Bun, and Cloudflare Workers (or anywhere else a native
Node addon is inconvenient), the WebAssembly binding at [bindings/wasm](bindings/wasm/)
publishes to npm as `@0xdoublesharp/bin-packing-wasm`. It ships combined and
dimension-specific targets through a single package:

```sh
pnpm add @0xdoublesharp/bin-packing-wasm
```

```js
// Bundlers (Vite, webpack, esbuild, Next.js, Rollup, Parcel)
import { solve1d, solve2d } from '@0xdoublesharp/bin-packing-wasm';

// Smaller dimension-specific browser bundles
import { solve1d as solveCutList } from '@0xdoublesharp/bin-packing-wasm/one-d';
import { solve2d as solveSheets } from '@0xdoublesharp/bin-packing-wasm/two-d';
import { solve3d as packBoxes } from '@0xdoublesharp/bin-packing-wasm/three-d';

// Raw ES modules / Deno — explicit init required
import init, { solve2d } from '@0xdoublesharp/bin-packing-wasm/web';
await init();

// Node.js / Bun — synchronous load, no init
import { solve1d } from '@0xdoublesharp/bin-packing-wasm/nodejs';
```

The API is identical to the Node binding: plain JS objects go in, plain JS
objects come out, and errors throw native JavaScript exceptions with the
original `BinPackingError` message. Full TypeScript types are included. The
combined output is ~550 KB of `wasm-opt -Oz`-optimized WebAssembly (1D+2D+3D); the
dimension-specific browser outputs are currently ~200 KB for 1D, ~230 KB for 2D, and ~300 KB for 3D. These are approximate — run the build locally for current figures.

Build it locally:

```sh
cd bindings/wasm
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
pnpm run build  # builds combined and dimension-specific targets into dist/
pnpm test       # runs the Node smoke test against dist/nodejs
```

See [bindings/wasm/README.md](bindings/wasm/README.md) for the full API
reference and per-target usage examples.

## Repository layout

```
crates/bin-packing/          # core solver crate
  src/one_d/                 #   1D model, heuristics, and exact backend
  src/two_d/                 #   2D model, MaxRects, Skyline, Guillotine, Shelf, RotationSearch
  src/three_d/               #   3D model, 29-algorithm catalog
  benches/solver_benches.rs  #   Criterion benchmarks (1D + 2D + 3D)
  tests/solver_regressions.rs
bindings/node/               # napi-rs Node.js bindings (@0xdoublesharp/bin-packing)
bindings/wasm/               # wasm-bindgen WebAssembly bindings (@0xdoublesharp/bin-packing-wasm)
fuzz/fuzz_targets/           # cargo-fuzz / libFuzzer targets (1D + 2D + 3D)
AGENTS.md                    # contributor rules
```

## Verification

Core Rust checks (matches CI gate described in [AGENTS.md](AGENTS.md)):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
```

Criterion benchmarks:

```sh
cargo bench -p bin-packing --bench solver_benches -- --sample-size 10
```

Node binding smoke test:

```sh
cd bindings/node
pnpm test
```

WebAssembly binding build and smoke test:

```sh
cd bindings/wasm
pnpm run build
pnpm test
```

Fuzz target build and execution:

```sh
cargo check --manifest-path fuzz/Cargo.toml
cargo fuzz run solver_inputs
```
