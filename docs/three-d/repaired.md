# Joint target-bin repair

`three_d::solve_3d_repaired(problem, options)` is an opt-in Rust entry point.
It first calls the selected ordinary solver, then destroys a target bin and
jointly repacks its contents with up to three destination bins. It preserves
existing bin instances, inventory, allowed rotations and the ordinary ranking.
Equal-quality rearrangements may be explored, but the returned incumbent never
ranks worse than the original result.

Repair tries at most `min(multistart_runs, 32) * min(improvement_rounds, 32)`
neighborhoods, with at least one start when rounds are nonzero. Neighborhoods
above 48 items are skipped. Each attempts at most two greedy repacks across at
most four bins and six orientations per item. These are additional work bounds;
the initial ordinary solve has its own cost.

Guillotine-required jobs skip repair because the arbitrary extreme-point
arrangements cannot certify guillotine cuts. The original relaxed-inventory
procurement estimates are preserved. A repaired layout is a heuristic incumbent,
not a global optimality or runtime guarantee. The function is not a selectable
`ThreeDAlgorithm` and is not exported by the JavaScript bindings.

See [the experiment findings](../experiments/2026-10-14-packing-findings.md) and
the implementation in [mod.rs](../../crates/bin-packing/src/three_d/mod.rs) and
[local_search.rs](../../crates/bin-packing/src/three_d/local_search.rs).

Effective option defaults are recorded in the [generated reference](../reference.md).
