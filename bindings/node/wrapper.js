'use strict';

const native = require('./index.js');

function solve1d(problem, options) {
  if (!native.solve1D) throw new Error('solve1d is not available in this build (missing one-d feature)');
  return JSON.parse(native.solve1D(JSON.stringify(problem), JSON.stringify(options ?? {})));
}

function solve2d(problem, options) {
  if (!native.solve2D) throw new Error('solve2d is not available in this build (missing two-d feature)');
  return JSON.parse(native.solve2D(JSON.stringify(problem), JSON.stringify(options ?? {})));
}

function solve3d(problem, options) {
  if (!native.solve3D) throw new Error('solve3d is not available in this build (missing three-d feature)');
  return JSON.parse(native.solve3D(JSON.stringify(problem), JSON.stringify(options ?? {})));
}

module.exports = {
  solve1d,
  solve2d,
  solve3d,
  solve1D: solve1d,
  solve2D: solve2d,
  solve3D: solve3d,
  version: native.version
};
