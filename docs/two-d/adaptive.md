# Adaptive rectangular packing

`two_d::adaptive::solve_2d_adaptive` runs the existing solvers sequentially and
retains their best result. `lower_bound_2d` supplies a conservative geometric
sheet-count bound. Once a complete packing meets that bound, the default mode
stops before the remaining algorithms.

This certifies **sheet count only**. It does not certify stock cost, waste across
different stock sizes, or reusable offcuts. Set `stop_on_count_bound: false` to
continue searching those secondary metrics. The ordinary `solve_2d` Auto API is
unchanged. With all eleven candidates enabled, the unconstrained ensemble
contains the same algorithms as ordinary Auto, though its ordering differs.

```js
const result = solve2dAdaptive(problem, {
  max_candidates: 4,
  stop_on_count_bound: true,
  solver: { seed: 42, auto_rotation_search_max_types: 4 },
});
console.log(result.solution.layouts, result.lower_bound, result.count_optimal);
```

The JavaScript names are `solve2dAdaptive` and `lowerBound2d` in native Node,
combined WASM and 2D-only WASM. Rust callers use `AdaptiveOptions2D` and
`AdaptiveSolution2D` from `two_d::adaptive`.

The candidate limit is between 1 and 11. Guillotine-required jobs use their
seven-algorithm ensemble. Inputs accept at most 4,096 parts, 128 demand types and
32 stocks; beam width is 1..=32, multistart runs 1..=64, and the rotation-search
threshold is at most 16. Unspecified seeds become zero.

The budget counts whole algorithm calls. It is neither a wall-clock deadline
nor a bound on the work inside a candidate. Validation and bound calculation
precede the ensemble. No count certificate is returned for a partial packing,
including when inventory prevents fulfilling the order.
