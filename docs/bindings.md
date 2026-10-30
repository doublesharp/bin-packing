# JavaScript API guide

The native package and combined WASM build expose the same plain-object packing
and planning APIs. Node uses `require('@0xdoublesharp/bin-packing')`; WASM Node
uses `require('@0xdoublesharp/bin-packing-wasm/nodejs')`. Browser `/web` imports
require `await init()` first. Bundler imports require WebAssembly ESM support in
the bundler. See the [generated export matrix](reference.md) for the exact APIs
available in each dimension-specific entry point.

Requests use the Rust JSON field names (`snake_case`), while JavaScript functions
use camel case. Every function is synchronous after initialization. Put large
jobs in a worker and enforce application runtime limits outside the solver.
Native and WASM accept the same models; seeded randomized layouts are reproducible
within one backend/toolchain, and can differ between architectures.

## Choose a model

| Need | API / guide |
| --- | --- |
| Ordinary cuts, rectangles or boxes | `solve1d`, `solve2d`, `solve3d`; dimension guides in the [index](README.md) |
| Lowest material price for linear stock | `solve1dMinCost`; [1D minimum cost](one-d/min-cost.md) |
| Rectangular trims/defects | `solve2dConstrained`; [2D constraints](two-d/constrained.md) |
| Cuboid payload limits | `solve3dConstrained`; [3D constraints](three-d/constrained.md) |
| Stop when minimum sheet count is attained | `solve2dAdaptive`, `lowerBound2d`; [adaptive search](two-d/adaptive.md) |
| Profit, mandatory quantities, feasibility or open size | `solve1dOptimized`, `solve2dOptimized`, `solve3dOptimized`; [objectives](objectives.md) |
| Physical staged guillotine tree | `solve2dStaged`, `solve2dStagedOptimized`; [staged cutting](two-d/staged.md) |
| Upright nesting, stack rules, axles or unloading | `solve3dLoading`, `solve3dLoadingOptimized`; [loading](three-d/loading.md) |
| Polygons, holes, circles or arcs | `solveIrregular`; [irregular nesting](irregular.md) |
| Legacy sequence/cost after packing | `plan1dCuts(problem, solution, options?)`, `plan2dCuts(solution, options?)` |
| Workpiece-aware schedule for an existing layout | `plan2dSawCuts(job, options?)`; [saw planner](two-d/saw-planner.md) |

Rust-only bounded entry points such as arc-flow, pattern recombination, mixed
rotation, bin repair and forecast reuse are documented in the dimension guides.
They are not algorithm strings accepted by the ordinary JavaScript solvers.

## Linear cuts and an ordered plan

Keep the original problem: the 1D cut planner needs its stock kerf. It omits a
final separating cut when the last piece exactly exhausts usable stock.

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solve1d, plan1dCuts } = require('@0xdoublesharp/bin-packing');
const problem = {
  stock: [{ name: 'bar', length: 10, kerf: 1 }],
  demands: [{ name: 'rail', length: 4, quantity: 2 }],
};
const solution = solve1d(problem, { algorithm: 'first_fit_decreasing' });
const plan = plan1dCuts(problem, solution, { preset: 'chop_saw' });
assert.equal(solution.stock_count, 1);
assert.equal(plan.bar_plans[0].num_cuts, 2);
```

## Select valuable demand under limited stock

Optimized 2D/3D requests wrap a constrained model, so `packing.problem` holds the
ordinary geometry. Optimized 1D uses the ordinary model directly as `packing`.
Profit and minimum arrays index the original demand rows. Duplicate labels are
allowed here; source indices identify the selected rows.

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solve2dOptimized } = require('@0xdoublesharp/bin-packing');
const result = solve2dOptimized({
  packing: { problem: {
    sheets: [{ name: 'sheet', width: 10, height: 1, quantity: 1 }],
    demands: [
      { name: 'same', width: 6, height: 1, quantity: 1, can_rotate: false },
      { name: 'same', width: 5, height: 1, quantity: 2, can_rotate: false },
    ],
  } },
  profits: [100, 1],
}, { objective: 'knapsack', max_evaluations: 32, packing_max_states: 64 });
assert.deepEqual(result.report.selected_quantities, [1, 0]);
assert.equal(result.report.total_profit, 100);
assert.equal(result.report.complete, false);
```

A partial knapsack result is expected when the objective prefers a subset.
`minimums_satisfied`, `complete` and `primary_optimal` answer different questions;
read the [objective report contract](objectives.md) before treating a result as a proof.

## Pack and retain the physical cut tree

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solve2dStaged } = require('@0xdoublesharp/bin-packing');
const result = solve2dStaged({
  packing: { problem: {
    sheets: [{ name: 'sheet', width: 10, height: 10 }],
    demands: [{ name: 'panel', width: 5, height: 10, quantity: 2, can_rotate: false }],
  } },
  rules: { max_stages: 1, first_axis: 'vertical', cut_type: 'exact' },
}, { max_states: 64 });
assert.equal(result.solution.sheet_count, 1);
assert.deepEqual(result.unplaced_quantities, [0]);
assert.equal(result.trees.length, 1);
```

The staged tree starts after border preparation. It differs from planning saw
operations on a pre-existing layout. Never infer equipment access or a universal
machine safety certificate from a guillotine flag.

## Load nested upright items

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solve3dLoading } = require('@0xdoublesharp/bin-packing');
const result = solve3dLoading({
  packing: {
    problem: {
      bins: [{ name: 'carton', width: 2, height: 9, depth: 2, quantity: 1 }],
      demands: [{ name: 'nested', width: 2, height: 6, depth: 2, quantity: 2, allowed_rotations: 1 }],
    },
    item_weights: [3],
    bin_max_weights: [6],
  },
  item_rules: [{ stackability_id: 7, nesting_height: 3, maximum_stackability: 2 }],
}, { max_states: 64 });
assert.equal(result.bin_count, 1);
assert.equal(result.layouts[0].stacks[0].height, 9);
assert.equal(result.layouts[0].payload, 6);
```

The vertical axis is Y. Nesting explicitly permits the modeled vertical overlap;
an ordinary cuboid overlap checker does not validate these results. Axle and
unloading rules are optional and must be supplied when the application needs them.

## Nest source geometry

<!-- bin-packing:example -->
```js
const assert = require('node:assert/strict');
const { solveIrregular } = require('@0xdoublesharp/bin-packing');
const circle = radius => ({ components: [{
  exterior: { kind: 'circle', center: { x: 0, y: 0 }, radius },
}] });
const result = solveIrregular({
  bins: [{ name: 'round-stock', shape: circle(5), quantity: 1 }],
  demands: [{ name: 'disc', shape: circle(2), quantity: 1 }],
}, { max_states: 64 });
assert.equal(result.feasible, true);
assert.deepEqual(result.unplaced_quantities, [0]);
assert.equal(result.layouts[0].placements[0].demand_index, 0);
```

Placements transform original local source geometry: reflection, rotation, then
translation. They do not describe bounding-box origins. Curved mixed geometry
uses a conservative approximation; circle-to-circle predicates are analytic.

## Validation, precision and exports

Numeric inputs must be finite. Weights, seeds and integer loading limits must
fit nonnegative JavaScript safe integers. Omit optional limits or use `null` where
the declaration permits it; an infinite weight limit is invalid. Large area and
volume summaries use ordinary JSON-number precision. Unknown fields in new
physical/objective request objects are rejected.

Object APIs throw JavaScript errors when validation or solving fails. Rust
structured error variants are not separate JavaScript error classes. Partial
packing is represented in results, and a bounded-search shortfall does not prove
infeasibility. Source-index maps remain authoritative when labels repeat.

Combined WASM exports JSON-string fallbacks for ordinary solves and irregular
nesting. Specialized imports retain their corresponding object APIs, and omit
JSON fallbacks. `version()` reports the compiled package version. The generated
reference is checked against declarations; packed runtime/type/browser contracts
separately verify that shipped implementations match those declarations.
