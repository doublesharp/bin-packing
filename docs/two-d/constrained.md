# Constrained rectangular packing

`two_d::constrained::solve_2d_constrained` is an additive native Rust API for
rectangular sheet packing with damaged material, trimmed borders and an explicit
material objective. Existing `solve_2d` inputs and ranking are unchanged.

`ConstrainedProblem2D` contains the original `TwoDProblem` and either one
`SheetConstraints2D` per stock type or an empty constraints vector. Borders and
defects use original sheet coordinates. Defects may overlap each other and the
trimmed borders, but must have positive dimensions and lie within the original
sheet. Trims must leave a positive rectangular usable region. Every stock copy
inherits its type's exclusions.

The new API uses physical sheet bounds. It rejects `edge_kerf_relief = true`
rather than allowing parts to extend past the stock. Rectangles are separated
from other parts and defects by at least the declared kerf along one axis. No
extra kerf is charged between a part and the trimmed usable border. This is a
conservative clearance model, not a staged cutting plan: it does not guarantee
guillotine separability, a maximum number of cutting stages, or machine access
around a defect.

The objective applies while selecting placements and stocks:

- `bin_count`: minimize unplaced quantity, sheet count, unused original-sheet
  area, then material cost.
- `total_cost`: minimize unplaced quantity, material cost, sheet count, then
  unused original-sheet area.

Both objectives prioritize fulfillment before material optimization. Limited
stock may produce a valid partial result. The deterministic greedy baseline is
retained unless a completed search candidate improves the requested objective.
`max_states = 0` returns the baseline. The state budget bounds generated beam
and bin-elimination successors, not wall time, baseline work, candidate
enumeration or validation.
An effort or beam-width cutoff sets `search_truncated`; no global optimality
claim is made even when this is false, because placement anchors and demand
order are restricted.

The beam starts with decreasing-area item order. Its per-depth branching reserves
effort for completion and merges equivalent indexed geometry. It includes a
charged skip alternative even for a currently fitting part, allowing finite stock
to fulfill two smaller parts instead of one larger part. Frontier selection keeps
distinct parents before extra geometric siblings so tied descendants do not
erase every other construction family.

For count-first, fully placed jobs of at most 64 parts and 16 demand types that
initially require multiple sheets, three quarters of the state budget is reserved
for bin elimination. The ordinary iterative demand-order repair prefers repeated
parts capable of filling an exact-height or exact-width strip, then explores
other demand types and free corners. It uses exact unions of overlapping maximal
free rectangles for capacity pruning and keeps at most 4,096 visited geometry
keys. Its depth is bounded by 64 parts and its successor shortlist by
`beam_width`. Partial and cost-first jobs use the fixed-order beam lane.

When eliminating to two sheets, an additional native lane first assigns source
quantities between the sheets. It then tries full or one-vacancy grids of the
largest repeated source and an iterative canonical first-free-cell search. A
successful first pattern is paired with an independently constructed complement.
This removes cross-sheet item-order permutations from the geometric search.
Only zero-kerf stock without trims or defects enters this lane; other inputs
retain the physical constrained fallback. Both sheets retain their source types,
costs, legal rotations and inventory limits.

The lane accepts at most 4,096 source-quantity combinations and 4,096 cells after
dividing dimensions by their common GCD. Scaling an identical job by ten therefore
preserves its compact grid. Allocation-prefix checks are bounded by
`16 * max_states`; retained profiles are capped at the smaller of 1,024 and one
quarter of the state budget. Every retained profile, canonical root, seed copy,
placement and explicit void successor is charged to `max_states`. Forced unusable
regions propagate within each charged geometry node. Per-seed continuation is
limited to eight nodes and complement continuation to 128; the iterative stack is
capped at 256. Omissions set `search_truncated`, and detailed preprocessing/call
counters appear in metrics. The first improved complete packing ends repair;
this remains a heuristic.

The elongated comparison regression uses 11 parts on a 60 by 20 sheet: three
21 by 6, four 17 by 5, two 9 by 8 and two 33 by 4 rectangles, with rotation allowed
for the first and third types. The previous greedy/fixed-order search used two
sheets. The demand-order repair produces one independently validated sheet at
beam width 8 and `max_states = 512`. This is a targeted regression gain rather
than a general benchmark claim.

The dense mixed-rectangle regression, formerly using three sheets even at 100,000
states, now uses two at a 4,000-state budget. Its 15 parts fill
840 of 864 available area units. An independently validated ten-times-scaled copy
also uses two sheets within that budget.

Source indices are authoritative: `stock_indices` maps layouts to stock types
and `demand_indices` maps each placement to its original demand. Duplicate stock
or demand labels are therefore supported. Layouts retain their construction
order so these maps remain aligned. Unplaced records retain their complete
source dimensions and rotation permissions.

`validate_2d_constrained` checks provenance, original bounds, trims, defect and
part clearance, legal orientations, stock caps, exact placed-plus-unplaced
accounting, and reported material totals. Waste includes trims and defects.
The existing kerf cut-line metric only counts matching placement edges; it does
not estimate cuts used to remove defective or trimmed material. Reusable-drop
metrics exclude defect and border material.

The API is deliberately bounded to keep search and validation suitable for
WebAssembly: at most 512 parts, 128 demand types, 16 stock types, 256 total
defects, beam width 1 through 64, and 100,000 search successors. Aggregate area must
fit in `u64` and aggregate cost must remain finite. Input or control values
outside those limits fail with a structured `InvalidConstraint`; invalid output
fails with `InvalidSolution`.

The geometry search stores source indices instead of cloning labels while
probing placements. Free-space subtraction uses O(F²) containment pruning for F
free rectangles. Output readback checks every pair of placements; reusable-drop
metrics use the existing O(P³) geometry helper for P placements and exclusions
on a sheet. Repair capacity checks use an O(F² log F) union sweep over F maximal
free rectangles. The state budget does not bound those auxiliary costs.

Effective option defaults are recorded in the [generated reference](../reference.md).
