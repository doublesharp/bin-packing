// Run after building bindings: node examples/saw-planner.mjs [native|wasm|two-d]
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const target = process.argv[2] ?? 'native';
const api = require(target === 'native' ? '../bindings/node/wrapper.js'
  : target === 'two-d' ? '../bindings/wasm/dist/two-d/nodejs/bin_packing_wasm.cjs'
  : '../bindings/wasm/dist/nodejs/bin_packing_wasm.cjs');
const problem = {
  sheets: [{ name: 'plywood', width: 1209, height: 600, kerf: 3 }],
  demands: [{ name: 'panel', width: 300, height: 600, quantity: 4, can_rotate: false }],
};
const solution = api.solve2d(problem, { algorithm: 'guillotine' });
// This example has exactly one stock and demand type, so index 0 is unambiguous.
const job = { problem, layouts: solution.layouts,
  stock_indices: solution.layouts.map(() => 0),
  demand_indices: solution.layouts.map(l => l.placements.map(() => 0)) };
const plan = api.plan2dSawCuts(job, { objective: 'weighted_effort' });
console.log(JSON.stringify({ counts: plan.counts, effort: plan.effort_score, search: plan.search,
  operations: plan.operations }, null, 2));
