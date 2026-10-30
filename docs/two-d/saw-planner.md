# Workpiece-aware saw planning

`two_d::saw_plan::plan_saw_cuts` and JavaScript `plan2dSawCuts(job, options)` turn existing layouts into a physical table-saw or panel-saw job. This is an opt-in API. Packing objectives, `plan2dCuts`, CNC paths, and 1D/3D APIs retain their behavior.

## Input and provenance

Supply `SawJob2D` with the original `problem`, geometry-only `layouts`, `stock_indices`, and `demand_indices`. There must be one source stock index per layout and one source demand index per placement. Names are labels. Duplicate names neither merge stock instances nor identify demands.

JavaScript can pass `solution.layouts` directly because the new decoder ignores packing metrics. Rust callers convert each layout with `SawLayout2D::from(&layout)`. This avoids exposing unused `u128` metrics to JSON number conversion. The original problem supplies kerf, inventory, dimensions and rotation constraints. Input validation checks those constraints and mapped usage; partial packing solutions are supported, and only their supplied placements are scheduled. The planner does not infer missing source indices. Retain provenance when constructing layouts, or supply an independently verified mapping. The legacy solver does not yet return source indices, so ambiguous old results need caller-provided provenance before migration.

A job accepts at most 128 sheets and 512 placements. These limits bound validation and search memory on WASM. Source stock labels may repeat in this API even though the legacy packing solver rejects duplicate stock names. The new validator normalizes labels only in a validation copy and retains all physical checks.

## Physical machine model

All rectangles and diagram segments use the original sheet frame: origin at top left, X right, Y down. Workpieces have a stable identity consisting of sheet-instance index and bounds. Finished parts use sheet-instance and placement indices. Root stock starts at orientation zero. Children inherit the orientation at their creating cut. Turning material in the machine does not alter the layout or its `rotated` flag.

For the machine frame, the guide is on the left and the blade travels along Y. Put the named original-workpiece edge against the guide. Clockwise quarter turns are Left=0, Top=3, Right=2, Bottom=1. A rotation instruction sets that absolute orientation. One orientation change is one rotation operation, including a half turn. No through-thickness flips are modeled.

The table-saw preset requires a rip fence for original-sheet vertical cuts and a separate crosscut sled with a stop for original-sheet horizontal cuts. Crosscuts never use the rip fence. The panel-saw preset assumes a reconfigurable panel stop and material that can be turned in-plane. These are abstract accessory models with no machine-specific width, length, clamp, support or minimum-workpiece capacity limits. The caller must select equipment that supports the listed operations. CNC routers, stacked cutting, bevels, changing blades and machines lacking these accessories are outside this API. An unsupported machine string is rejected. A job containing different source kerfs is rejected; split it into separate blade jobs.

Each cut's `setting` is the current workpiece's reference-side material width to the nearest blade edge, excluding kerf. It is never an absolute sheet coordinate. The reference side must contain material. `kerf_region` describes consumed material and `segment` follows its low-coordinate blade edge over the full current workpiece. The far blade edge is determined by the region and stock kerf. Use the band for diagrams that show blade width.

Each selected workpiece has an explicit `handle` operation, including initial loading. Handling means select/load the workpiece; clearing and putting aside the resulting pieces is included in the pass. A child that needs another cut gets its own later load. Handling weight therefore scores one load per physical pass in this version. It does not estimate carrying distance or independent offcut transfers. Workpieces left whole need no machine operation.

The active setup is accessory plus distance. The first use of each accessory is an `initial_setup`. Changing distance or returning to a previously used accessory is a `setup_change`. A first use of a second accessory counts only as its initial setup. State survives rotations, loads and sheet changes; switching axes does not erase setup history. Repeated ready operations at one setup are sequential passes, never stacked or simultaneous cutting.

## Kerf, edges and failures

A cut consumes its parent exactly once and yields the listed nonempty children plus the blade band. Every required part must remain wholly in a child; the blade cannot intrude into one. Exact finished rectangles and empty waste rectangles are terminal. Factory edges need no cut. A gap exactly one kerf wide disappears in the separating pass, with no phantom second pass. Larger gaps can leave real offcuts that require a second pass when both boundaries are finished-part edges. `usable_offcut` classifies terminal waste using `min_usable_side` on both dimensions.

When edge relief is enabled, a blade may extend beyond the current workpiece, with only the intersecting material reported as kerf loss. Finished parts may never extend beyond physical source stock. The old packing model can produce such overhangs when `edge_kerf_relief` is enabled; this planner returns `InvalidGeometry` for them. It does not silently enlarge stock or shrink parts.

Nonintrusive cuts can still leave an untrimmable margin when full kerf clearance is required. Baseline construction backtracks over these choices. A 13×12 sheet with kerf 2 and 4×4 parts at (0,0) and (7,6), for example, requires avoiding a vertical-first cut that strands a one-unit margin.

Rust failures use the separate, non-exhaustive `SawPlanError2D`: invalid problem, options, mapping, geometry, non-guillotine layout, or baseline construction limit. Bindings throw an exception with the error message. `NonGuillotine` means the physical boundary-cut model could not finish that sheet. `BaselineLimit` means construction ran out of effort before obtaining a valid incumbent; it is not proof of infeasibility.

## Objectives and search

- `fewest_setup_changes` minimizes initial setups plus subsequent setup changes.
- `fewest_cuts` minimizes physical passes.
- `weighted_effort` minimizes `cuts × cut + initial_setups × initial_setup + setup_changes × setup_change + rotations × rotation + handling × handling`.

Ties use weighted effort, cuts, total setups, rotations, handling, then stable candidate order. Initial setups count toward the setup objective because opening another accessory is real setup work. Effective default weights are in the [generated reference](../reference.md#sawweights2d). Weights must be finite and between 0 and 1e100. They are abstract effort units, separate from material cost; the score is not estimated time.

The deterministic algorithm:

1. Construct a valid baseline with bounded depth-first backtracking over full-span cuts at finished boundaries. Score both retained reference edges without constructing successors, then try each geometric split lazily using its cheaper edge. Reference-edge choice affects effort but cannot change decomposition feasibility. Process sheets in input order.
2. Reschedule the baseline tree's ready operations using actual incremental effort, allowing cross-sheet choices only when batching is enabled. Reconstruct only its known cuts and construct a successor only for the selected move. This produces a completed candidate before broader exploration on larger jobs.
3. Run a beam search over ready workpieces, alternative decompositions and both reference edges. Keep the best completed schedule under the selected objective's full tie-breaking rules.

The baseline remains an incumbent. For weighted effort, the result's score cannot exceed `baseline_effort_score`. For the other objectives, the guarantee is lexicographic: a lower primary count may legitimately have higher effort. Abandoned partial states never replace a completed schedule.

Default search settings are in the [generated reference](../reference.md#sawsearchlimits2d). Allowed maxima are 100,000 baseline split attempts, 100,000 optimization evaluations and beam width 256. `max_states: 0` returns the validated baseline. Construction failure is explicit; optimization budget exhaustion returns the best completed incumbent. Baseline attempts include candidates rejected for intersecting a required part; scoring reference edges does not consume that budget. Optimization evaluations include dry-scored alternatives in the preliminary tree scheduling pass. Time spent validating, enumerating boundaries or alternative geometry, and serializing is outside those counts, so this is a deterministic effort bound, not a wall-clock deadline.

For p contained parts, baseline boundary enumeration and scoring cost O(p log p), with O(p) work to validate and partition each attempted split. A successful strip decomposition therefore avoids constructing all O(p) alternatives at every step. Broader beam-search geometry queries still cost O(p²), producing O(p) candidates. A bounded cache reuses those queries across machine states. It retains at most 4,096 workpieces and 65,536 estimated storage units; larger queries are used without caching. Beam search ranks lightweight moves before allocating successor states; retained moves share only their selected split. Search traces share prefixes, ready states share immutable workpieces, and strings remain in the input. Baseline traversal is iterative to avoid recursion proportional to the cut count.

`heuristic` means a beam or budget limit prevented complete exploration. `optimal_within_candidate_model` is returned only if the complete finite search was exhausted without beam pruning or budget truncation. That model permits full-span cuts flush to finished boundaries with either blade side and reference edge. It does not enumerate arbitrary auxiliary waste cuts, machine-specific strategies, stacks, or alternative packings. There is no claim of a global shop minimum.

## Output and Subchecker migration

`SawPlan2D` schema version 1 contains one authoritative `operations` list, all `workpieces`, per-sheet reference views, physical counts, effective weights, effort score and search evidence. Operation IDs are indices and are deterministic for identical input/options. Workpiece and finished-part identities are independent of labels and schedule order. `depends_on` identifies the cut that created a workpiece; machine-state dependencies additionally require execution in the complete job order.

Render cut bands and segments directly. Do not run the legacy phantom-gap filter on this output. Reference instructions should include workpiece bounds, named edge, accessory, distance, kerf band, and child identities. Keep material totals on the packing result. Show initial setups and changes separately, and label effort as effort.

`sheet_views[].operation_ids` reference the authoritative schedule for diagrams. They are not standalone execution plans. In particular, interleaved sheets must be executed in job order to obtain the reported setup savings. Batching changes scheduling, not placement orientation or physical pass concurrency.

This API adds new Rust types and functions without adding fields or variants to the existing packing or cutting structs/enums. Existing Rust literals and legacy JSON shapes remain intact. New JavaScript exports exist in native Node, combined WASM and the two-d WASM entry point. This API is introduced in the additive 0.4.0 release. Future changes to required new Rust fields or exhaustive enums must account for source compatibility even if JSON gains defaults; incompatible output changes need another schema version.

Runnable examples:

```sh
cargo run -p bin-packing --example saw_planner --locked
node examples/saw-planner.mjs native
node examples/saw-planner.mjs wasm
node examples/saw-planner.mjs two-d
```

Build the Node and WASM bindings first using their existing `build` scripts. See [measured schedules and performance](saw-planner-results.md), [test audit and updated benchmarks](saw-planner-test-audit.md) and [future packing search design](effort-aware-packing.md).

Effective option defaults are recorded in the [generated reference](../reference.md).
