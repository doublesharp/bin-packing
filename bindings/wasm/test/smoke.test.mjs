import test from 'node:test';
import assert from 'node:assert/strict';

import { createRequire } from 'node:module';

// The nodejs target is CommonJS (wasm-pack's nodejs output). We rename it to
// `.cjs` during build so Node's loader treats it correctly under
// `"type": "module"`. Pulling it in via createRequire sidesteps any
// interop-default edge cases.
const require = createRequire(import.meta.url);
const wasm = require('../dist/nodejs/bin_packing_wasm.cjs');
const wasmOneD = require('../dist/one-d/nodejs/bin_packing_wasm.cjs');
const wasmTwoD = require('../dist/two-d/nodejs/bin_packing_wasm.cjs');
const wasmThreeD = require('../dist/three-d/nodejs/bin_packing_wasm.cjs');

test('solve1d returns a feasible cut list', () => {
  const solution = wasm.solve1d(
    {
      stock: [{ name: 'bar', length: 100, kerf: 1 }],
      demands: [
        { name: 'A', length: 45, quantity: 2 },
        { name: 'B', length: 30, quantity: 2 },
      ],
    },
    { algorithm: 'auto' },
  );

  assert.ok(typeof solution.stock_count === 'number');
  assert.equal(solution.unplaced.length, 0);
  assert.ok(solution.stock_count >= 1);
  assert.ok(Array.isArray(solution.layouts));
  assert.ok(solution.layouts.length >= 1);
});

test('one-d subpath exposes only the 1D object API', () => {
  const solution = wasmOneD.solve1d(
    {
      stock: [{ name: 'bar', length: 100, kerf: 1 }],
      demands: [{ name: 'A', length: 45, quantity: 2 }],
    },
    { algorithm: 'auto' },
  );

  assert.ok(solution.stock_count >= 1);
  assert.equal(solution.unplaced.length, 0);
  assert.equal(typeof wasmOneD.solve2d, 'undefined');
  assert.equal(typeof wasmOneD.solve1dJson, 'undefined');
});

test('solve2d returns a feasible sheet layout', () => {
  const solution = wasm.solve2d(
    {
      sheets: [{ name: 'plywood', width: 96, height: 48 }],
      demands: [
        { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true },
      ],
    },
    { algorithm: 'auto', seed: 42 },
  );

  assert.ok(typeof solution.sheet_count === 'number');
  assert.equal(solution.unplaced.length, 0);
  assert.ok(solution.sheet_count >= 1);
  assert.equal(solution.layouts[0].placements.length, 4);
});

test('two-d subpath exposes only the 2D object API', () => {
  const solution = wasmTwoD.solve2d(
    {
      sheets: [{ name: 'plywood', width: 96, height: 48 }],
      demands: [{ name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true }],
    },
    { algorithm: 'auto', seed: 42 },
  );

  assert.ok(solution.sheet_count >= 1);
  assert.equal(solution.unplaced.length, 0);
  assert.equal(typeof wasmTwoD.solve1d, 'undefined');
  assert.equal(typeof wasmTwoD.solve2dJson, 'undefined');
});

test('solve1d is reproducible under the same seed', () => {
  const problem = {
    stock: [{ name: 'bar', length: 50 }],
    demands: [
      { name: 'A', length: 12, quantity: 3 },
      { name: 'B', length: 8, quantity: 3 },
    ],
  };
  const options = { algorithm: 'local_search', seed: 7 };

  const first = wasm.solve1d(problem, options);
  const second = wasm.solve1d(problem, options);

  assert.deepEqual(first, second);
});

test('solve2d is reproducible under the same seed', () => {
  const problem = {
    sheets: [{ name: 'sheet', width: 20, height: 20 }],
    demands: [
      { name: 'a', width: 5, height: 5, quantity: 4, can_rotate: true },
      { name: 'b', width: 3, height: 7, quantity: 2, can_rotate: true },
    ],
  };
  const options = { algorithm: 'multi_start', seed: 11, multistart_runs: 4 };

  const first = wasm.solve2d(problem, options);
  const second = wasm.solve2d(problem, options);

  assert.deepEqual(first, second);
});

test('solve1d surfaces validation errors as thrown exceptions', () => {
  assert.throws(
    () =>
      wasm.solve1d(
        { stock: [], demands: [{ name: 'A', length: 10, quantity: 1 }] },
        {},
      ),
    /invalid input/i,
  );
});

test('solve2d surfaces infeasible demands as thrown exceptions', () => {
  assert.throws(
    () =>
      wasm.solve2d(
        {
          sheets: [{ name: 's', width: 5, height: 5 }],
          demands: [{ name: 'big', width: 10, height: 10, quantity: 1 }],
        },
        {},
      ),
    /no feasible sheet/i,
  );
});

test('solve1dJson and solve2dJson accept and return JSON strings', () => {
  const oneD = wasm.solve1dJson(
    JSON.stringify({
      stock: [{ name: 'bar', length: 100 }],
      demands: [{ name: 'A', length: 50, quantity: 2 }],
    }),
    JSON.stringify({ algorithm: 'auto' }),
  );
  const parsedOneD = JSON.parse(oneD);
  assert.ok(parsedOneD.stock_count >= 1);

  const twoD = wasm.solve2dJson(
    JSON.stringify({
      sheets: [{ name: 'sheet', width: 10, height: 10 }],
      demands: [{ name: 'x', width: 5, height: 5, quantity: 2, can_rotate: false }],
    }),
    undefined,
  );
  const parsedTwoD = JSON.parse(twoD);
  assert.ok(parsedTwoD.sheet_count >= 1);
});

test('version returns a non-empty string', () => {
  const version = wasm.version();
  assert.equal(typeof version, 'string');
  assert.ok(version.length > 0);
});

test('solve3d returns a feasible bin packing', () => {
  const solution = wasm.solve3d(
    {
      bins: [{ name: 'crate', width: 60, height: 40, depth: 30 }],
      demands: [
        { name: 'box_a', width: 10, height: 10, depth: 10, quantity: 3 },
        { name: 'box_b', width: 8, height: 6, depth: 5, quantity: 2 },
      ],
    },
    { algorithm: 'auto' },
  );

  assert.ok(typeof solution.bin_count === 'number');
  assert.equal(solution.unplaced.length, 0);
  assert.ok(solution.bin_count >= 1);
  assert.ok(Array.isArray(solution.layouts));
  assert.ok(solution.layouts.length >= 1);
  assert.equal(solution.layouts[0].placements.length, 5, 'all five boxes placed in first bin');
});

test('three-d subpath exposes only the 3D object API', () => {
  const solution = wasmThreeD.solve3d(
    {
      bins: [{ name: 'crate', width: 60, height: 40, depth: 30 }],
      demands: [{ name: 'box', width: 10, height: 10, depth: 10, quantity: 2 }],
    },
    { algorithm: 'auto' },
  );

  assert.ok(solution.bin_count >= 1);
  assert.equal(solution.unplaced.length, 0);
  assert.equal(typeof wasmThreeD.solve1d, 'undefined');
  assert.equal(typeof wasmThreeD.solve2d, 'undefined');
  assert.equal(typeof wasmThreeD.solve3dJson, 'undefined');
});

test('solve3d is reproducible under the same seed', () => {
  const problem = {
    bins: [{ name: 'bin', width: 50, height: 50, depth: 50 }],
    demands: [
      { name: 'a', width: 10, height: 10, depth: 10, quantity: 4 },
      { name: 'b', width: 8, height: 6, depth: 5, quantity: 3 },
    ],
  };
  const options = { algorithm: 'multi_start', seed: 99, multistart_runs: 4 };

  const first = wasm.solve3d(problem, options);
  const second = wasm.solve3d(problem, options);

  assert.deepEqual(first, second);
});

test('solve3d surfaces infeasible demands as thrown exceptions', () => {
  assert.throws(
    () =>
      wasm.solve3d(
        {
          bins: [{ name: 'tiny', width: 5, height: 5, depth: 5 }],
          demands: [{ name: 'big', width: 10, height: 10, depth: 10, quantity: 1 }],
        },
        {},
      ),
    /no feasible bin/i,
  );
});

test('solve3dJson accepts and returns JSON strings', () => {
  const result = wasm.solve3dJson(
    JSON.stringify({
      bins: [{ name: 'crate', width: 60, height: 40, depth: 30 }],
      demands: [{ name: 'box', width: 10, height: 10, depth: 10, quantity: 2 }],
    }),
    JSON.stringify({ algorithm: 'auto' }),
  );
  const parsed = JSON.parse(result);
  assert.ok(parsed.bin_count >= 1);
});
