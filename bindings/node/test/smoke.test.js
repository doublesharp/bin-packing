const test = require('node:test');
const assert = require('node:assert/strict');

const binPacking = require('..');

test('version returns a non-empty string', () => {
  assert.ok(typeof binPacking.version() === 'string');
  assert.ok(binPacking.version().length > 0);
});

test('solve1d packs a basic cut list', () => {
  const solution = binPacking.solve1d(
    {
      stock: [{ name: 'bar', length: 100 }],
      demands: [
        { name: 'A', length: 45, quantity: 2 },
        { name: 'B', length: 30, quantity: 2 }
      ]
    },
    { algorithm: 'auto' }
  );

  assert.equal(solution.stock_count, 2);
  assert.equal(solution.stock_requirements[0].required_quantity, 2);
  assert.equal(solution.unplaced.length, 0);
});

test('solve3d packs a basic 3D problem', () => {
  const solution = binPacking.solve3d(
    {
      bins: [{ name: 'crate', width: 60, height: 40, depth: 30 }],
      demands: [
        { name: 'box_a', width: 10, height: 10, depth: 10, quantity: 3 },
        { name: 'box_b', width: 8, height: 6, depth: 5, quantity: 2 }
      ]
    },
    { algorithm: 'auto' }
  );

  assert.ok(solution.bin_count >= 1, 'should use at least one bin');
  assert.equal(solution.unplaced.length, 0, 'all items should be placed');
  assert.equal(solution.layouts[0].placements.length, 5, 'all five boxes placed in first bin');
});

test('solve2d packs a basic sheet layout', () => {
  const solution = binPacking.solve2d(
    {
      sheets: [{ name: 'plywood', width: 96, height: 48 }],
      demands: [
        { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true }
      ]
    },
    { algorithm: 'auto', seed: 42 }
  );

  assert.ok(solution.sheet_count >= 1, 'should use at least one sheet');
  assert.equal(solution.unplaced.length, 0, 'all panels should be placed');
  assert.equal(solution.layouts[0].placements.length, 4, 'all four panels on one sheet');
  assert.ok(typeof solution.total_waste_area === 'number', 'waste area should be a number');
});
