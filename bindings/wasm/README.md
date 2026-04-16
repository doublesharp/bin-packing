<p align="center">
  <img src="https://raw.githubusercontent.com/doublesharp/bin-packing/main/docs/bin-packing.png" alt="bin-packing" width="200">
</p>

# @0xdoublesharp/bin-packing-wasm

WebAssembly build of the [`bin-packing`](https://crates.io/crates/bin-packing)
Rust crate — a cut list and bin packing optimizer for 1D cutting stock (linear
bar / pipe), 2D rectangular sheet packing, and 3D box packing.

Runs in **browsers, Node.js 18+, Deno, Bun, and Cloudflare Workers**. No
native binaries, no build step, no `node-gyp`.

## Features

- **1D cutting stock.** First-fit-decreasing, best-fit-decreasing, multistart
  local search, and an exact column-generation backend.
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

The package ships combined targets plus smaller dimension-specific targets via
the `exports` map, so a single install works everywhere:

| Import specifier | Target | Requires `init()`? |
| ---- | ---- | ---- |
| `@0xdoublesharp/bin-packing-wasm` | Combined 1D+2D+3D, bundlers (Vite, webpack, esbuild, Next.js, Rollup, Parcel) | No |
| `@0xdoublesharp/bin-packing-wasm/web` | Combined 1D+2D+3D, browsers, Deno, native ES modules | Yes (`await init()`) |
| `@0xdoublesharp/bin-packing-wasm/nodejs` | Combined 1D+2D+3D, Node.js, Bun | No |
| `@0xdoublesharp/bin-packing-wasm/one-d` | 1D-only, bundlers | No |
| `@0xdoublesharp/bin-packing-wasm/one-d/web` | 1D-only, browsers, Deno, native ES modules | Yes (`await init()`) |
| `@0xdoublesharp/bin-packing-wasm/one-d/nodejs` | 1D-only, Node.js, Bun | No |
| `@0xdoublesharp/bin-packing-wasm/two-d` | 2D-only, bundlers | No |
| `@0xdoublesharp/bin-packing-wasm/two-d/web` | 2D-only, browsers, Deno, native ES modules | Yes (`await init()`) |
| `@0xdoublesharp/bin-packing-wasm/two-d/nodejs` | 2D-only, Node.js, Bun | No |
| `@0xdoublesharp/bin-packing-wasm/three-d` | 3D-only, bundlers | No |
| `@0xdoublesharp/bin-packing-wasm/three-d/web` | 3D-only, browsers, Deno, native ES modules | Yes (`await init()`) |
| `@0xdoublesharp/bin-packing-wasm/three-d/nodejs` | 3D-only, Node.js, Bun | No |

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
//   .steps        — ordered cut steps (cut, rotate, fence_reset, …)
//   .total_cost   — cost for this sheet
//   .num_cuts, .num_rotations, .num_fence_resets, .num_tool_ups, .travel_distance
```

Modern bundlers load the accompanying `.wasm` file automatically. Vite and
webpack 5+ need no configuration; older toolchains may need a wasm loader.

For browser payloads that only need one dimension, import the smaller
dimension-specific entry point:

```js
import { solve2d } from '@0xdoublesharp/bin-packing-wasm/two-d';
```

The 1D-only WASM is currently ~200 KB, the 2D-only WASM is currently ~230 KB,
the 3D-only WASM is currently ~300 KB, and the combined 1D+2D+3D WASM is
currently ~550 KB after `wasm-opt -Oz`. These are approximate; run the build
locally for current figures.

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
    { algorithm: 'auto', seed: 42, min_usable_side: 12 },
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

```js
const { solve1d } = require('@0xdoublesharp/bin-packing-wasm/nodejs');
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

### `plan1dCuts(solution, options?)`

```ts
function plan1dCuts(solution: OneDSolution, options?: CutPlanOptions1D): CutPlanSolution1D;
```

Generates an ordered cut plan for every bar in a finished `OneDSolution`. The
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
- `column_generation` — exact backend; reports `exact: true` and a
  `lower_bound` when optimal

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

- Browser WASM is ~1.5–3× slower than native for CPU-bound integer code.
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

## License

MIT.
