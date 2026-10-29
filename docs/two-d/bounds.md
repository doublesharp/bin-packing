# Conservative sheet-count lower bounds

`two_d::lower_bound_2d(&problem)` validates the ordinary 2D problem and returns a
lower bound on the number of sheets required to place every demanded part.
It does not expand quantities or solve the packing problem.

For multiple stock types, it divides the total demanded area by the largest
effective sheet area and rounds up. Inventory is relaxed. When edge kerf relief
is enabled, effective area includes the solver's allowed boundary extension.
An individually oversized demand is rejected using the ordinary fit rules.

For one stock type without edge relief, the bound also evaluates a bounded
family of conservative scales on each axis:

- Identity.
- Integer floor scales: map `x` to `floor(x / q)` and axis capacity `C` to
  `floor(C / q)`, using up to eight divisors `q = floor(C / (k + 1)) + 1`.
- Threshold scales: for `1 <= t <= C / 2`, map `x < t` to zero, `x > C - t`
  to `C`, and other lengths to themselves, using up to eight thresholds.

Each scale preserves every feasible one-dimensional interval chain. For a floor
scale, the sum of floors cannot exceed the floor of the original sum. For a
threshold scale, a chain containing a length greater than `C - t` has less than
`t` total length left, so its other intervals disappear; otherwise no interval
is enlarged. Applying a preserving scale to each axis and compacting along the
original interval precedence graphs preserves a rectangular packing.

The transformed total area divided by transformed sheet area is therefore a
valid lower bound. A rotatable demand contributes the minimum transformed area
over its allowed orientations that fit the original sheet. This is conservative
even when different copies use different rotations.

For example, two copies each of fixed-orientation `6x4`, `4x6`, and `4x3` parts
in `10x6` sheets have an area lower bound of two sheets. The geometric bound
raises this to three. It does not produce a layout or certify feasibility.

The implementation is original Rust, informed by the published conservative-scale
argument and [PackingSolver's explanation of dual feasible functions](https://fontanf.github.io/packingsolver/internals/rectangle/dual_feasible_functions.html).
It is a bounded subset of transformations rather than a claim of equal coverage.
At most 17 scales per axis produce 289 combinations: O(289 * demand types) time
and O(demand types) memory, independent of quantities. Aggregate area uses `u128`.
Blade gaps and inventory caps are relaxed, so the bound does not establish that
the available stock can fulfill the order. Equality with a feasible solution
certifies only minimum sheet count; leftover quality and cost need their own
proof.
