<p align="center">
  <img src="https://raw.githubusercontent.com/doublesharp/bin-packing/main/docs/bin-packing.png" alt="bin-packing" width="200">
</p>

# @0xdoublesharp/bin-packing-wasm

WebAssembly build of the [`bin-packing`](https://crates.io/crates/bin-packing)
Rust crate — a cut list and bin packing optimizer for 1D cutting stock (linear
bar / pipe), 2D rectangular sheet packing, 3D box packing, staged cutting, stack loading and native irregular nesting.

Provides browser and Node.js targets without native binaries or `node-gyp`.
See the [generated engine and export reference](https://github.com/doublesharp/bin-packing/blob/v0.6.0/docs/reference.md)
for the Node engine range. Deno, Bun and worker-host integrations may require
loader configuration and are not covered by the Node/Chromium qualification.

## Features

- **1D cutting stock.** First-fit-decreasing, best-fit-decreasing, multistart
  local search, and a restricted column-generation backend that proves count
  optimality only after complete pattern enumeration and integer search.
- **2D rectangular packing.** MaxRects (best-area, BSSF, BLSF, bottom-left,
  contact-point), Skyline (+ min-waste), Guillotine beam search (7 variants),
  shelf heuristics (NFDH, FFDH, BFDH), multistart, and rotation search
  meta-strategies.
- **3D box packing.** Extreme Points (6 variants), Guillotine 3D beam search
  (7 variants), layer/wall/column builders, Deepest-Bottom-Left, volume-sorted
  FFD/BFD, MultiStart, GRASP, LocalSearch, and a restricted branch-and-bound
  exact backend (29-algorithm catalog). Per-item rotation masks over all six
  axis permutations.
- **Multi-stock / multi-sheet / multi-bin** with independent costs and optional
  inventory caps.
- **Kerf and trim** modeling for 1D cuts; **kerf-aware gap enforcement** for 2D sheet packing.
- **Per-item rotation** control for 2D demands and 3D rotation masks.
- **`Auto` mode** runs multiple strategies and returns the best candidate.
- **Reproducible** — all randomized strategies accept a `seed`.
- **Fully typed** TypeScript definitions.

## Installation

```sh
pnpm add @0xdoublesharp/bin-packing-wasm
# or
npm install @0xdoublesharp/bin-packing-wasm
# or
yarn add @0xdoublesharp/bin-packing-wasm
```

## Usage

See the [complete JavaScript API guide](https://github.com/doublesharp/bin-packing/blob/v0.6.0/docs/bindings.md)
for physical/objective examples and the
[generated reference](https://github.com/doublesharp/bin-packing/blob/v0.6.0/docs/reference.md)
for exports, defaults and supported entry points.

The explicit-objective APIs are `solve1dOptimized`, `solve2dOptimized` and
`solve3dOptimized`. Requests contain `{ packing, profits?, minimum_quantities? }`;
options select `bin_count`, `total_cost`, `knapsack`, `feasibility` or supported
`open_dimension_*` objectives. Results retain the effective source problem,
indexed stock/demand provenance, native solution and an objective report.
Unplaced copies and bounded-search failures are not infeasibility proofs.

`solve2dStaged` returns physical staged guillotine cut trees, including kerf,
trims, defects, machine distances/counts and cutting groups.
`solve2dStagedOptimized` applies explicit objectives to that model.
`solve3dLoading` handles compatible nested stacks, payloads, stack limits,
density, axle reactions and unloading rules; `solve3dLoadingOptimized` adds
explicit objectives. `solveIrregular` nests rigid polygons, holes, circles and
arcs with permitted rotations/reflection and spacing. Original source indices
remain authoritative when labels repeat.

Every numeric input must be finite. Weights, density fractions, axle limits,
stack load limits and seeds must be nonnegative JavaScript safe integers.
Optional limits may be omitted or explicitly `null` where declared nullable;
non-finite values and undefined array elements are rejected before JSON
serialization. Unsafe aggregate output loads are rejected. Large area/volume
summary metrics continue to use ordinary JSON-number precision.

The additional APIs are `solve1dMinCost`, `solve2dConstrained`,
`solve3dConstrained`, `solve2dAdaptive` and `lowerBound2d`. Dimension-specific
WASM builds expose the corresponding dimension's APIs. Constrained requests
wrap an ordinary problem in `{ problem, ...constraints }`: 2D adds indexed
sheet trims and rectangular defects; 3D adds item weights and maximum payloads.
Weights must be nonnegative JavaScript safe integers; only explicit `null`
means an unlimited payload. Cost-first objectives prioritize demand fulfillment.
Budgets count algorithm calls or successor states, rather than wall time;
adaptive count certificates cover sheet count only. Existing solvers keep their
ranking.

Irregular-only imports are `@0xdoublesharp/bin-packing-wasm/irregular`,
`/irregular/web` (initialize first) and `/irregular/nodejs`. They expose
`solveIrregular` and `version` without rectangular solvers or JSON-string
helpers. The full build also includes `solveIrregularJson`.

The package ships combined targets plus smaller dimension-specific targets via
the `exports` map, so a single install works everywhere:

<!-- BEGIN GENERATED: wasm-imports -->
| Import | Target / initialization |
| --- | --- |
| `@0xdoublesharp/bin-packing-wasm` | bundler: WASM ESM support |
| `@0xdoublesharp/bin-packing-wasm/web` | web: await init() |
| `@0xdoublesharp/bin-packing-wasm/nodejs` | nodejs: synchronous |
| `@0xdoublesharp/bin-packing-wasm/one-d` | bundler: WASM ESM support |
| `@0xdoublesharp/bin-packing-wasm/one-d/web` | web: await init() |
| `@0xdoublesharp/bin-packing-wasm/one-d/nodejs` | nodejs: synchronous |
| `@0xdoublesharp/bin-packing-wasm/two-d` | bundler: WASM ESM support |
| `@0xdoublesharp/bin-packing-wasm/two-d/web` | web: await init() |
| `@0xdoublesharp/bin-packing-wasm/two-d/nodejs` | nodejs: synchronous |
| `@0xdoublesharp/bin-packing-wasm/three-d` | bundler: WASM ESM support |
| `@0xdoublesharp/bin-packing-wasm/three-d/web` | web: await init() |
| `@0xdoublesharp/bin-packing-wasm/three-d/nodejs` | nodejs: synchronous |
| `@0xdoublesharp/bin-packing-wasm/irregular` | bundler: WASM ESM support |
| `@0xdoublesharp/bin-packing-wasm/irregular/web` | web: await init() |
| `@0xdoublesharp/bin-packing-wasm/irregular/nodejs` | nodejs: synchronous |
<!-- END GENERATED: wasm-imports -->

### Bundler (Vite / webpack / esbuild / Next.js)

```js
import { solve1d, solve2d, plan2dCuts } from '@0xdoublesharp/bin-packing-wasm';

const cutList = solve1d(
  {
    stock: [{ name: 'bar', length: 100, kerf: 1 }],
    demands: [
      { name: 'A', length: 45, quantity: 2 },
      { name: 'B', length: 30, quantity: 2 },
    ],
  },
  { algorithm: 'auto' },
);

console.log(cutList.stock_count, cutList.total_waste);

// Generate a cut plan from a finished 2D layout
const layout = solve2d(
  {
    sheets: [{ name: 'plywood', width: 96, height: 48, kerf: 2 }],
    demands: [
      { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true },
    ],
  },
  { algorithm: 'auto' },
);

const cutPlan = plan2dCuts(layout, { preset: 'table_saw' });

// cutPlan.total_cost — aggregate cost across all sheets
// cutPlan.sheet_plans — per-sheet plans; each carries:
//   .steps        — ordered cut steps (cut, line_cut, rotate, fence_reset, …)
//   .total_cost   — cost for this sheet
//   .num_cuts, .num_rotations, .num_fence_resets, .num_tool_ups, .travel_distance
```

The bundler target imports its `.wasm` module. Enable WebAssembly ESM support
in your bundler; some setups require a WASM plugin or configuration. Use the
`/web` target with explicit initialization when loading the module by URL.

For browser payloads that only need one dimension, import the smaller
dimension-specific entry point:

```js
import { solve2d } from '@0xdoublesharp/bin-packing-wasm/two-d';
```

Payload size depends on the selected models, target and toolchain. The
[dated expansion report](https://github.com/doublesharp/bin-packing/blob/v0.6.0/docs/experiments/2026-10-28-packingsolver-completion.md)
records measured sizes; build the current tree to measure a new release.

### Browser (raw ES modules / Deno)

The `web` target needs an explicit `init()` call that loads the `.wasm` file
from a URL:

```html
<script type="module">
  import init, { solve2d } from 'https://unpkg.com/@0xdoublesharp/bin-packing-wasm/dist/web/bin_packing_wasm.js';

  await init();

  const layout = solve2d(
    {
      sheets: [{ name: 'plywood', width: 96, height: 48, kerf: 2 }],
      demands: [
        { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true },
      ],
    },
    { algorithm: 'auto', guillotine_required: true, seed: 42, min_usable_side: 12 },
  );

  console.log(layout.sheet_count, layout.total_waste_area);
</script>
```

In Deno:

```ts
import init, { solve1d } from 'npm:@0xdoublesharp/bin-packing-wasm/web';

await init();
const solution = solve1d(/* ... */);
```

### Node.js / Bun

```js
import { solve1d } from '@0xdoublesharp/bin-packing-wasm/nodejs';

const solution = solve1d(
  {
    stock: [{ name: 'bar', length: 100 }],
    demands: [{ name: 'A', length: 45, quantity: 2 }],
  },
  { algorithm: 'auto' },
);
```

The Node target loads the `.wasm` file synchronously from disk — no async
setup required.

### CommonJS

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solve1d, plan1dCuts, version } = require('@0xdoublesharp/bin-packing-wasm/nodejs');
const problem = {
  stock: [{ name: 'bar', length: 10 }],
  demands: [{ name: 'rail', length: 5, quantity: 2 }],
};
const solution = solve1d(problem, { algorithm: 'first_fit_decreasing' });
assert.equal(solution.stock_count, 1);
assert.equal(plan1dCuts(problem, solution).bar_plans[0].num_cuts, 1);
assert.match(version(), /^\d+\.\d+\.\d+$/);
```

### Edge kerf relief

Set `edge_kerf_relief: true` on a sheet when the final cut on each
axis can run off the stock, consuming less than a full kerf of
material:

```js
sheets: [
  {
    name: 'plywood',
    width: 48000,
    height: 96000,
    kerf: 125,
    edge_kerf_relief: true,
  },
];
```

Each part must still fit within the sheet's own dimensions, but the
last placement on a row or column may extend by up to one kerf past
the sheet boundary — the model treats this as the blade exiting the
material.

## API

### `plan2dCuts(solution, options?)`

```ts
function plan2dCuts(solution: TwoDSolution, options?: CutPlanOptions2D): CutPlanSolution2D;
```

Generates an ordered cut plan for every sheet in a finished `TwoDSolution`.
Presets (`options.preset`): `table_saw`, `panel_saw`, `cnc_router`.

The returned plan carries per-sheet steps and a `total_cost`. Each entry in
`sheet_plans` includes an ordered `steps` array and counters for cuts,
rotations, fence resets, tool ups, and total travel distance.

Throws when `table_saw` or `panel_saw` is used on a non-guillotine layout
(`NonGuillotineNotCuttable`) or when a cost override is invalid
(`InvalidOptions`). Use `cnc_router` as the universal fallback for any layout.

### `plan1dCuts(problem, solution, options?)`

```ts
function plan1dCuts(problem: OneDProblem, solution: OneDSolution, options?: CutPlanOptions1D): CutPlanSolution1D;
```

Generates an ordered cut plan for every bar in a finished `OneDSolution`.
Pass the original problem as the first argument so kerf comes from the stock
definitions. The
only preset is `chop_saw`. Each `bar_plans` entry carries ordered `steps`
(`cut` and `fence_reset`) and a `total_cost`.

### `solve1d(problem, options?)`

```ts
function solve1d(problem: OneDProblem, options?: OneDOptions): OneDSolution;
```

Solve a 1D cutting-stock problem. Throws a JavaScript `Error` on validation
failures, infeasible demands, or unsupported solver configurations.

Algorithms (`options.algorithm`):
- `auto` *(default)* — runs FFD, BFD, local search, and optionally escalates
  to column generation
- `first_fit_decreasing`
- `best_fit_decreasing`
- `local_search` — multistart; honors `seed`, `multistart_runs`, `improvement_rounds`
- `column_generation` — single uncapped stock type only; other configurations
  throw. `exact: true` requires complete pattern enumeration and integer search.
  An optimal result found under a capped enumeration can still be non-exact;
  `lower_bound` is a certified LP or conservative length/capacity bound.

### `solve2d(problem, options?)`

```ts
function solve2d(problem: TwoDProblem, options?: TwoDOptions): TwoDSolution;
```

Solve a 2D rectangular bin-packing problem. Throws on validation failures or
infeasible demands.

Algorithms (`options.algorithm`):
- `auto` *(default)* — runs the full ensemble
- MaxRects: `max_rects`, `max_rects_best_short_side_fit`, `max_rects_best_long_side_fit`,
  `max_rects_bottom_left`, `max_rects_contact_point`
- Skyline: `skyline`, `skyline_min_waste`
- Guillotine beam search: `guillotine`, `guillotine_best_short_side_fit`,
  `guillotine_best_long_side_fit`, `guillotine_shorter_leftover_axis`,
  `guillotine_longer_leftover_axis`, `guillotine_min_area_split`,
  `guillotine_max_area_split`
- Shelf heuristics: `next_fit_decreasing_height`, `first_fit_decreasing_height`,
  `best_fit_decreasing_height`
- Meta-strategies: `multi_start`, `rotation_search`

Set `options.guillotine_required = true` to restrict `auto` to
guillotine-compatible constructions.

### `solve3d(problem, options?)`

```ts
function solve3d(problem: ThreeDProblem, options?: ThreeDOptions): ThreeDSolution;
```

Solve a 3D rectangular bin-packing problem. Throws a JavaScript `Error` on
validation failures, infeasible demands, or unsupported solver configurations.

Algorithms (`options.algorithm`):
- `auto` *(default)* — runs a tiered ensemble of algorithms and returns the
  best result
- Extreme Points: `extreme_points`, `extreme_points_residual_space`,
  `extreme_points_free_volume`, `extreme_points_bottom_left_back`,
  `extreme_points_contact_point`, `extreme_points_euclidean`
- Guillotine 3D: `guillotine_3d`, `guillotine_3d_best_short_side_fit`,
  `guillotine_3d_best_long_side_fit`, `guillotine_3d_shorter_leftover_axis`,
  `guillotine_3d_longer_leftover_axis`, `guillotine_3d_min_volume_split`,
  `guillotine_3d_max_volume_split`
- Layer building: `layer_building`, `layer_building_max_rects`,
  `layer_building_skyline`, `layer_building_guillotine`, `layer_building_shelf`
- Geometry: `wall_building`, `column_building`, `deepest_bottom_left`,
  `deepest_bottom_left_fill`, `first_fit_decreasing_volume`,
  `best_fit_decreasing_volume`
- Meta-strategies: `multi_start`, `grasp`, `local_search`, `branch_and_bound`

### JSON-string fallbacks

`solve1dJson(problemJson, optionsJson?)`, `solve2dJson(problemJson, optionsJson?)`,
and `solve3dJson(problemJson, optionsJson?)` accept and return JSON strings
directly. Useful when the caller already has a JSON payload in hand (HTTP
request body, file read, worker message). These are exported by the combined
entry points; the dimension-specific entry points keep only the plain-object
API to reduce browser payload size.

### `version()`

Returns the package version string.

## TypeScript types

All input and output types are fully typed. Import them from the package:

```ts
import type {
  OneDProblem,
  OneDSolution,
  OneDOptions,
  TwoDProblem,
  TwoDSolution,
  TwoDOptions,
  Placement2D,
  ThreeDProblem,
  ThreeDSolution,
  ThreeDOptions,
  Placement3D,
} from '@0xdoublesharp/bin-packing-wasm';
```

## Performance notes

- Native and WASM runtime varies with workload and host. Use dated measurements
  for timing claims, and benchmark the selected model in the target environment.
- The default `Auto` mode runs many strategies — for interactive UIs, pick a
  specific fast algorithm like `first_fit_decreasing_height` or
  `max_rects_best_short_side_fit`.
- Consider running solves inside a Web Worker for large problems.
- Reproducibility: pass `options.seed` to get deterministic output across
  runs.

## Building from source

```sh
git clone https://github.com/doublesharp/bin-packing
cd bin-packing/bindings/wasm

# One-time prerequisites
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# Build combined and dimension-specific bundler, web, and nodejs targets into dist/
pnpm run build

# Run the Node smoke test
pnpm test
```

## Workpiece-aware saw jobs

`plan2dSawCuts(job, options?)` is available in combined and `two-d` builds for every
target. It accepts explicit stock/demand provenance and returns one authoritative
physical schedule. See [the API and migration guide](https://github.com/doublesharp/bin-packing/blob/v0.6.0/docs/two-d/saw-planner.md).
After building, run `node ../../examples/saw-planner.mjs wasm` or use `two-d`.

## License

MIT.
