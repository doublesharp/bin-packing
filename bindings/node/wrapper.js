'use strict';

const native = require('./index.js');

// Reject invalid numeric values before JSON can coerce them to null.
const safeIntegerFields = new Set(['maximum_weight_above', 'numerator', 'denominator', 'front_maximum', 'rear_maximum', 'front_tare', 'rear_tare', 'seed']);
function safeInteger(value, nullable) {
  if (nullable && value === null) return;
  if (!Number.isSafeInteger(value) || value < 0) throw new Error('weights and loading limits must be nonnegative JavaScript safe integers');
}
function checkNumbers(value, field = '', ancestors = new Set(), depth = 0) {
  if (depth > 128) throw new Error('input exceeds the supported JSON nesting depth');
  if (safeIntegerFields.has(field)) safeInteger(value, field === 'maximum_weight_above' || field === 'seed');
  if ((field === 'item_weights' || field === 'bin_max_weights') && value !== undefined) {
    if (!Array.isArray(value)) throw new Error('weights must be arrays of nonnegative JavaScript safe integers');
    for (const item of value) safeInteger(item, field === 'bin_max_weights');
  }
  if (typeof value === 'number' && !Number.isFinite(value)) throw new Error('all numeric input fields must be finite');
  if (value && typeof value === 'object') {
    if (ancestors.has(value)) throw new Error('input must be JSON-serializable');
    ancestors.add(value);
    for (const [key, item] of Object.entries(value)) checkNumbers(item, key, ancestors, depth + 1);
    ancestors.delete(value);
  }
}
function stringify(value) {
  checkNumbers(value);
  return JSON.stringify(value, (_key, item) => {
    if (typeof item === 'number' && !Number.isFinite(item)) throw new Error('all numeric input fields must be finite');
    return item;
  });
}
function solveExtended(name, problem, options) {
  if (typeof native[name] !== 'function') throw new Error(`${name} is not available in this build`);
  return JSON.parse(native[name](stringify(problem), stringify(options ?? {})));
}
function solve1dOptimized(problem, options) { return solveExtended('solve1DOptimized', problem, options); }
function solve2dOptimized(problem, options) { return solveExtended('solve2DOptimized', problem, options); }
function solve3dOptimized(problem, options) { return solveExtended('solve3DOptimized', problem, options); }
function solve2dStaged(problem, options) { return solveExtended('solve2DStaged', problem, options); }
function solve2dStagedOptimized(problem, options) { return solveExtended('solve2DStagedOptimized', problem, options); }
function solve3dLoading(problem, options) { return solveExtended('solve3DLoading', problem, options); }
function solve3dLoadingOptimized(problem, options) { return solveExtended('solve3DLoadingOptimized', problem, options); }
function solveIrregular(problem, options) { return solveExtended('solveIrregular', problem, options); }

function solve1d(problem, options) {
  if (!native.solve1D) throw new Error('solve1d is not available in this build (missing one-d feature)');
  return JSON.parse(native.solve1D(stringify(problem), stringify(options ?? {})));
}

function solve1dMinCost(problem, options) {
  if (!native.solve1DMinCost) throw new Error('solve1dMinCost is not available in this build (missing one-d feature)');
  return JSON.parse(native.solve1DMinCost(stringify(problem), stringify(options ?? {})));
}

function solve2d(problem, options) {
  if (!native.solve2D) throw new Error('solve2d is not available in this build (missing two-d feature)');
  return JSON.parse(native.solve2D(stringify(problem), stringify(options ?? {})));
}

function solve3d(problem, options) {
  if (!native.solve3D) throw new Error('solve3d is not available in this build (missing three-d feature)');
  return JSON.parse(native.solve3D(stringify(problem), stringify(options ?? {})));
}

function solve2dConstrained(problem, options) {
  if (!native.solve2DConstrained) throw new Error('solve2dConstrained is not available in this build (missing two-d feature)');
  return JSON.parse(native.solve2DConstrained(stringify(problem), stringify(options ?? {})));
}

function solve3dConstrained(problem, options) {
  if (!native.solve3DConstrained) throw new Error('solve3dConstrained is not available in this build (missing three-d feature)');
  for (const weight of [...(problem.item_weights ?? []), ...(problem.bin_max_weights ?? []).filter(value => value !== null)]) {
    if (!Number.isSafeInteger(weight) || weight < 0) throw new Error('weights must be nonnegative JavaScript safe integers');
  }
  return JSON.parse(native.solve3DConstrained(stringify(problem), stringify(options ?? {})));
}

function solve2dAdaptive(problem, options) {
  if (!native.solve2DAdaptive) throw new Error('solve2dAdaptive is not available in this build (missing two-d feature)');
  return JSON.parse(native.solve2DAdaptive(stringify(problem), stringify(options ?? {})));
}

function lowerBound2d(problem) {
  if (!native.lowerBound2D) throw new Error('lowerBound2d is not available in this build (missing two-d feature)');
  return JSON.parse(native.lowerBound2D(stringify(problem)));
}

function plan1dCuts(problem, solution, options) {
  if (!native.plan1DCuts) throw new Error('plan1dCuts is not available in this build (missing one-d feature)');
  return JSON.parse(native.plan1DCuts(stringify(problem), stringify(solution), options != null ? stringify(options) : undefined));
}

function plan2dCuts(solution, options) {
  if (!native.plan2DCuts) throw new Error('plan2dCuts is not available in this build (missing two-d feature)');
  return JSON.parse(native.plan2DCuts(stringify(solution), options != null ? stringify(options) : undefined));
}

function plan2dSawCuts(job, options) {
  if (!native.plan2DSawCuts) throw new Error('plan2dSawCuts is not available in this build (missing two-d feature)');
  return JSON.parse(native.plan2DSawCuts(stringify(job), options != null ? stringify(options) : undefined));
}

module.exports = {
  solve1dOptimized, solve2dOptimized, solve3dOptimized, solve2dStaged, solve2dStagedOptimized,
  solve3dLoading, solve3dLoadingOptimized, solveIrregular,
  solve1dMinCost,
  solve2dConstrained,
  solve3dConstrained,
  solve2dAdaptive,
  lowerBound2d,
  plan2dSawCuts,
  solve1d,
  solve2d,
  solve3d,
  solve1D: solve1d,
  solve2D: solve2d,
  solve3D: solve3d,
  plan1dCuts,
  plan2dCuts,
  plan1d_cuts: plan1dCuts,
  plan2d_cuts: plan2dCuts,
  version: native.version
};
