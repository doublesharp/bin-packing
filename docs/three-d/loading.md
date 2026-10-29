# Stack loading in three dimensions

`three_d::loading::solve_loading_3d` constructs floor-supported, equal-footprint
columns in Rust. The model follows PackingSolver's boxstacks family: stackability
identifiers, nesting height, item count and weight-above limits, stack density,
payload, floor defects, two-support axle reactions and delivery groups. It uses
the existing source bins, demands, costs, rotation masks and weight arrays from
`ConstrainedProblem3D`.

The vertical axis is **Y**; the floor is **X/Z**. Only the upright `xyz` and `zyx`
rotations are considered, subject to each source demand's mask. A mask containing
neither upright rotation is rejected. Columns can contain different source
demands when their rotated footprints and `stackability_id` agree. Every demand
and bin retains its original vector index, including duplicate demand names.

## Rules and defaults

`LoadingProblem3D` wraps `packing`, `item_rules`, `bin_rules` and `unloading`.
Each rule vector must be empty or have exactly one entry per corresponding source
type. An empty item-rule vector means identifier zero, no nesting, no stack-count
or weight-above limit, and group zero. Empty bin rules mean no density limit,
axle limits or floor defects. Empty source weight arrays mean zero item weight
and no payload limit, as in the constrained model. Source stock quantity and
geometry validation still apply. Invalid constraints fail before search.

Each `StackItemRules3D` supports:

- `stackability_id`: equal values are required throughout one column.
- `nesting_height`: height saved when this item is placed above another item.
  The bottom item starts at Y=0 and receives no saving. An upper item's bottom
  is the previous column top minus its own nesting height. Its full physical
  height then determines the new top. Nesting must be less than the source item
  height and cannot place an item below the floor.
- `maximum_stackability`: maximum **total item count** in any column containing
  this item, including items below and above it. A specified limit must be
  positive.
- `maximum_weight_above`: checked sum of the source weights of **all** higher
  items, independently for every item in the column.
- `group_id`: smaller groups unload first.

`LoadingBinRules3D::maximum_stack_density` uses an exact `WeightDensity3D`
fraction. For numerator `p`, positive denominator `q`, column weight `w` and
footprint area `a`, the constraint is `w*q <= p*a`, evaluated with widened integer
arithmetic. It applies to the whole column. Floor defects are positive rectangles
inside the source floor; their interiors exclude all column footprints.

The nesting representation deliberately permits physical vertical overlap
authorized by these rules. The ordinary 3D non-overlap validator cannot validate
such columns. Use `validate_loading_3d`, which independently reconstructs the
nesting, full top height, source orientation, loads and quantities.

## Axles and delivery

`AxleLimits3D` specifies two support positions along X or Z, total reaction caps
and fixed tare reactions. Positions must increase within the source bin, and
each tare must fit its cap. For supports `f < r`, cargo mass `W`, cargo moment
`M`, and span `L=r-f`, the exact reactions are:

```text
front = front_tare + (r*W-M)/L
rear  = rear_tare  + (M-f*W)/L
```

An item's horizontal center is the center of its column. Twice-moments preserve
half-unit centers exactly; `i128` arithmetic avoids floating-point load decisions.
Both reactions must be nonnegative and at most their declared caps. Reported
`AxleLoads3D` values are integer ceilings of those exact reactions, so their sum
may exceed payload plus tare by a rounding unit. No rounding is used to make an
overloaded reaction feasible.

This is a generic static two-support model. Applications with articulated
tractor/trailer geometry must supply a suitable support model and tare reactions;
the API does not infer a vehicle's suspension or load transfer from its name.

With any active unloading rule, every remaining-delivery-group suffix must also
satisfy the axle limits. A balanced complete shipment is rejected if removal of
an earlier group leaves an invalid axle load. The solver checks every committed
construction prefix as well, so intermediate cargo additions satisfy the model.

Unloading is toward the **positive** chosen axis:

- `only_x_movements` and `only_z_movements` prevent a later group from blocking
  an earlier group when their column footprints overlap along the perpendicular
  floor axis. The earlier column must be entirely beyond the later column along
  the unloading axis.
- `increasing_x` and `increasing_z` require the origin coordinate of an earlier
  group to be at least the origin coordinate of a later group, across all lanes.
  Equal origins are permitted.
- All active modes require group IDs to be nonincreasing from bottom to top
  within each column, allowing earlier groups to be lifted off first.

These rules describe floor lanes and stack removal. They do not model doors,
lifting equipment, travel paths around obstacles, acceleration, tipping,
material compression, general overhang support or other mechanical stability.

## Search and evidence

`LoadingOptions3D` supports `bin_count` and `total_cost`, after minimizing
unplaced quantity. Construction tries legal upright orientations, compatible
columns, maximal free floor rectangles and deterministic corner/center anchors.
Exact axle feasibility intervals add load-balancing anchors. A beam then explores
different source types, item order and placements. Source bin inventory is
consumed during construction. A complete, independently validated greedy
incumbent is retained even when search has no budget.

The explicit supported input bounds are 128 expanded items, 32 source demand
types, 16 source bin types and 256 total floor defects, in addition to the base
3D geometry bounds. Controls allow `beam_width` from 1 through 32 and
`max_states` from 0 through 100000. Free rectangles are capped at 128 and move
shortlists at 64. These caps can omit placements; they never authorize an
invalid one. Candidate anchors are finite rather than an enumeration of all
integer floor coordinates.

`max_states` counts evaluated cloned beam successors. Deterministic greedy
construction, geometry checks, candidate enumeration and independent validation
are outside that counter. Zero returns the completed baseline. Output always
accounts for every source item as either placed or explicitly unplaced.
`search_truncated` records budget or shortlist/beam/space omissions. It does not
claim optimality or prove infeasibility when an item remains unplaced.

The result contains `LoadingBinLayout3D` records with indexed source bins, checked
payload, columns and optional axle ceilings; `StackedItem3D` records carry source
demand indices, physical Y positions and rotations. The independent validator
also checks stock limits, source quantities, recorded geometry, counts, costs,
payloads, density, load propagation, delivery order and exact axle reactions.

```rust
use bin_packing::three_d::loading::{
    LoadingOptions3D, LoadingProblem3D, StackItemRules3D,
    UnloadingConstraint3D, solve_loading_3d, validate_loading_3d,
};
# use bin_packing::three_d::constrained::ConstrainedProblem3D;
# fn load(packing: ConstrainedProblem3D) -> bin_packing::Result<()> {
let loading = LoadingProblem3D {
    packing,
    item_rules: Vec::<StackItemRules3D>::new(),
    bin_rules: Vec::new(),
    unloading: UnloadingConstraint3D::OnlyZMovements,
};
let solution = solve_loading_3d(loading.clone(), LoadingOptions3D::default())?;
validate_loading_3d(&loading, &solution)?;
# Ok(())
# }
```
