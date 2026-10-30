# Payload-aware 3D packing

`three_d::constrained::solve_3d_constrained` adds payload limits and an explicit
material objective without changing `ThreeDProblem`, `ThreeDOptions` or ordinary
solver behavior. This is a native Rust implementation; it does not link C++.

Wrap an existing problem in `ConstrainedProblem3D`. `item_weights` indexes source
demands and contains the weight of one item, using any consistent integer unit.
`bin_max_weights` indexes source bin types and contains `Some(maximum_payload)` or
`None`. Empty arrays mean zero weights and unlimited payloads respectively;
nonempty arrays must match their source lengths. Empty cartons and padding must
be accounted for by the caller when deriving each allowed payload. Weight limits
are checked before placing an item, independently of geometric volume.

`ConstrainedOptions3D.objective` chooses `bin_count` (unplaced count, bin count,
empty volume, material cost) or `total_cost` (unplaced count, material cost, bin
count, empty volume). The objective affects construction and beam ranking.
Neither objective computes carrier rates or dimensional-weight pricing.

The result wraps a `ThreeDSolution` with `bin_indices`, `demand_indices` and
`bin_weights` in exactly the same layout/placement order. Demand names are labels;
repeated names are supported because weight, dimensions and conservation use
source indices. Unplaced items have quantity one and follow source-demand order.
Inventory shortages, geometrically impossible demands and overweight items
produce explicit partial results. No stock is purchased implicitly and the
ordinary solver's relaxed-inventory procurement estimate is not copied here.

## Construction and bounded improvement

Two deterministic constructors place decreasing-volume items into indexed
maximal free cuboids, retaining the better completed result under the selected
material objective. One favors tight individual free spaces; the other favors
utilized bins and low `(y, x, z)` corners, preserving larger contiguous regions
for later fixed-axis items. Cuboids may overlap each other, but never placed items;
their volumes are not added together as independent free capacity. Labels are
borrowed during probing and cloned only once per source demand and at final
output. A bounded beam then explores alternative corners, orientations and bin
types while retaining that incumbent. Reaching a search limit cannot make the
returned material objective worse than either constructor. A complete one-bin
packing with a single stock type already attains both supported full material
objectives: count is minimal, and cost/waste are fixed. The beam is skipped in
that case and `explored_states` is zero. This shortcut does not invoke the ordinary
Auto ensemble.

Default search settings are in the [generated reference](../reference.md#constrainedoptions3d). `max_states = 0` returns the
better constructor incumbent. Accepted maxima are 256 total items, 64 demand
types, 16 bin types, beam width 32 and 100,000 successor states. Each expansion retains at most
16 placement moves and each open bin at most 128 maximal free cuboids.
`explored_states` counts successor states, including explicit skipped-item
alternatives; construction, candidate probes, validation, serialization and wall
time are outside that effort count. Free-cuboid subtraction emits at most six
pieces per intersecting cuboid, followed by quadratic containment pruning.

`search_truncated` reports omitted alternatives from budgets, beam width or
geometry/move shortlists. A false value describes only the fixed-order corner
candidate model. The general search provides no global count or cost certificate.
`solution.exact` stays false even for the restricted single-bin shortcut, and
`lower_bound` is absent. It does not enforce
physical support, load-bearing, fragility, stacking, axle or unloading rules, and
does not certify guillotine compatibility.

Payload sums use checked `u64` arithmetic. Items whose combined weight exceeds
`u64::MAX` cannot share one bin even when no payload cap is declared. Inputs that
could overflow aggregate `f64` material cost are rejected conservatively.

## Independent readback

`validate_3d_constrained(&problem, &result)` checks source indices, dimensions,
rotation masks, bin boundaries, pairwise non-overlap, inventory, actual payload,
used/waste volumes, material totals and exact placed/unplaced demand conservation.
It also rejects unsupported exactness, lower-bound, guillotine or procurement
claims. Validation is quadratic in placements per bin and linear in remaining
source/metadata size. A malformed constraint returns structured
`BinPackingError::InvalidConstraint`; inconsistent output returns
`BinPackingError::InvalidSolution`.

Effective option defaults are recorded in the [generated reference](../reference.md).
