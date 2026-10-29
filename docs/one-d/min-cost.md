# Cost-first cutting stock

`one_d::solve_1d_min_cost(problem, options)` minimizes unplaced cut count, total
material cost, bar count, then unused usable length. JavaScript exposes
`solve1dMinCost` in native Node and combined/1D-only WebAssembly builds.

A length-10 bar costing 10 can fulfill two length-5 cuts in one bar. Two
length-5 bars costing 1 each fulfill the same order for 2. Cost-first search
can select those two bars; ordinary `solve_1d` continues to prioritize bar count.

This API uses the existing compressed pattern DAG and bounded integer search,
with kerf, trim, finite stock and repeated demand labels. Its objective affects
candidate comparison and pruning: a count-first incumbent never prevents the
search from exploring a cheaper solution with more bars. `algorithm` is
`arc_flow_min_cost`; `exact` certifies the selected cost-first objective only if
the full supported search completes. Budget exhaustion retains a complete
incumbent and leaves `exact` false. No material-cost lower bound is reported.

Limits match `solve_1d_arc_flow`: 128 demand/stock types, 4,096 cuts and 128
selected bars. `exact_pattern_limit` bounds graph storage, and its product with
`max(column_generation_rounds, 1)` bounds integer search work. The initial Auto
incumbent and relaxed-inventory procurement estimates are outside that budget;
the procurement estimate remains count-first, rather than a certified
cost-optimal purchasing recommendation. Aggregate material costs must satisfy a
conservative finite-`f64` ceiling before incumbent construction.

`options.algorithm` is ignored by this dedicated entry point; the other options
control the same pattern search used by `solve_1d_arc_flow`.

Fractional material costs disable numerical memoization, preserving the exact
public floating-point comparison rather than rounding prices. Small mixed-stock,
kerf, trim and inventory problems are checked against an independent per-piece
assignment oracle, including fractional prices and partial fulfillment.
