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

test('plan1dCuts generates a cut plan for a 1D solution', () => {
  const problem = {
    stock: [{ name: 'bar', length: 100, kerf: 2 }],
    demands: [
      { name: 'A', length: 30, quantity: 3 }
    ]
  };
  const solution = binPacking.solve1d(problem, { algorithm: 'auto' });
  const plan = binPacking.plan1dCuts(problem, solution);

  assert.ok(typeof plan.total_cost === 'number', 'total_cost should be a number');
  assert.ok(Array.isArray(plan.bar_plans), 'bar_plans should be an array');
  assert.ok(plan.bar_plans.length > 0, 'should have at least one bar plan');
  assert.equal(plan.preset, 'chop_saw', 'default preset should be chop_saw');
  assert.ok(typeof plan.effective_costs.cut_cost === 'number', 'effective_costs.cut_cost should be a number');
  assert.ok(Array.isArray(plan.bar_plans[0].steps), 'steps should be an array');
});

test('plan2dCuts generates a cut plan for a 2D solution', () => {
  const solution = binPacking.solve2d(
    {
      sheets: [{ name: 'plywood', width: 10, height: 5 }],
      demands: [{ name: 'panel', width: 5, height: 5, quantity: 2, can_rotate: false }]
    },
    { algorithm: 'guillotine' }
  );
  const plan = binPacking.plan2dCuts(solution);

  assert.ok(typeof plan.total_cost === 'number', 'total_cost should be a number');
  assert.ok(Array.isArray(plan.sheet_plans), 'sheet_plans should be an array');
  assert.ok(plan.sheet_plans.length > 0, 'should have at least one sheet plan');
  assert.equal(plan.preset, 'table_saw', 'default preset should be table_saw');
});
