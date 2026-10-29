# Explicit packing objectives

The additive `optimization` entry points keep source demand identity and physical
constraints while changing the primary objective:

| Rust entry point | Physical model |
| --- | --- |
| `one_d::optimization::solve_1d_optimized` | Linear cuts, blade loss, trim, stock price and inventory |
| `two_d::optimization::solve_2d_optimized` | Rectangles, rotation, blade gap, border trims and defects |
| `two_d::optimization::solve_2d_staged_optimized` | Authoritative staged physical cut trees and machine restrictions |
| `three_d::optimization::solve_3d_optimized` | Cuboids, legal rotations, payload and inventory |
| `three_d::optimization::solve_loading_3d_optimized` | Nested upright stack columns, load, axle and unloading rules |

Irregular nesting supplies equivalent objectives directly in
`irregular::IrregularOptions`, using its own floating-point geometry and work
limits. Its free-aspect-ratio open XY objective differs from the reference's
fixed-aspect-ratio open XY model.

Each regular entry point accepts `OptimizationProblem { packing, profits,
minimum_quantities }` and `OptimizationOptions`. Profit and minimum arrays are
indexed by the original demand vector. Empty profit arrays mean one per copy;
empty minima mean zero. Values must be finite and nonnegative. Minima cannot
exceed the corresponding requested quantities.
New objective, machine-rule and physical-constraint objects reject unknown JSON
fields so misspelled constraints cannot silently turn into defaults.

`BinCount` prioritizes fulfilled copies, container count, cost and waste.
`TotalCost` prioritizes fulfillment and material price. `Knapsack` prioritizes
satisfying mandatory quantities, then maximizes profit; it explores demand
subsets and calls the physical solver on each. An oversized optional demand can
remain unplaced while other demands are selected. A cutting group's later source
type can be selected only after every requested copy of its predecessors.
`Feasibility` seeks a complete packing within inventory. A partial result remains
a witness of the reported selected copies, never proof that full packing is
impossible.

`OpenDimensionX`, `OpenDimensionY`, `OpenDimensionZ` and `OpenDimensionXy` seek a
complete packing in one container with smaller declared dimensions. Only axes
present in that model are accepted. Exactly one source stock type is required;
its original dimensions are upper limits and at most one copy may be consumed.
XY minimizes the product of X and Y. Both orders of coordinate reduction are
tried; binary size exploration is a heuristic policy, since a failed bounded
packing attempt is not an infeasibility proof.

Open crops retain source coordinates, fixed trim widths, original part geometry,
load rules and axle positions. Defects are intersected with the cropped stock.
An axle support outside a trial crop makes that trial unusable. Nested loading
volume bounds discount nesting overlaps; ordinary cuboid volume is not a valid
bound for that model.

The result contains the effective physical input, source stock/demand indices,
native geometry and `OptimizationReport`. `selected_quantities` and
`total_profit` describe actual placed source copies, including duplicate labels.
The native unplaced list accounts for every omitted original copy. Reported
completeness and mandatory fulfillment are separate facts.
The optimized 1D result leaves legacy `stock_requirements` empty: a procurement
estimate from a selected subset would not describe the full original job. The
ordinary 1D entry point remains available for a full-job procurement estimate.

`primary_optimal` is true only when a complete witness meets a conservative
primary bound, or when every positive-profit copy is selected. It says nothing
about secondary cost, offcuts or layout optimality. Bounds describe the complete
requested problem; a partial witness's container count is not an optimal value
for fulfilling the full job. Large integer measures can round in floating-point
wire fields, while bound comparisons use widened integer arithmetic internally.

Search is deterministic and bounded to 128 copies, 16 demand types and 16 stock
types. `max_evaluations` permits 1–4096 complete physical solver calls and
`beam_width` permits 1–32 retained candidates. `packing_max_states` permits
0–100000 native work units per call. In 1D it caps DAG records and integer-search
work separately; in the other regular models it caps successor states. Baseline
construction, probes, independent validation and serialization are additional
work. Zero retains a construction incumbent. Truncation records omitted search
alternatives, and no wall-clock deadline is implied.
