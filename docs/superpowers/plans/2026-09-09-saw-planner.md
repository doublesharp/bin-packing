# Workpiece-aware saw planner implementation plan

Goal: implement the requested opt-in physical job planner without changing packing selection or legacy cutting APIs.

Architecture: a new `two_d::saw_plan` module owns a versioned request/output contract, input validation, kerf-aware full-workpiece split generation, and bounded scheduling. Explicit stock/demand indices preserve identities even when labels repeat. All coordinates stay in original sheet space; physical orientation and reference edges are separate.

The alternative of extending legacy structs breaks Rust literals and still lacks stock provenance. Sorting legacy steps cannot explore decompositions. A new API with dynamic split generation addresses both problems.

## Tasks

- [x] Add independent replay tests for repeated strips, kerf, dependencies, geometry, and identity. Run the tests before implementing.
- [x] Add documented new request, options, operation, workpiece, result and structured error types. Validate the original problem using index identities, mapped layouts, inventory, demand dimensions and rotation constraints.
- [x] Enumerate full-span cuts at finished boundaries with kerf on either side. Partition children and classify exact finished parts or offcuts. Reject physical overhang, intrusion, overlaps and unsupported machinery.
- [x] Build a bounded backtracking baseline, reschedule its ready operations, then run bounded beam search over ready workpieces, split choices and reference edges. Keep the completed baseline incumbent. Record explicit setup, loading and orientation operations and derive all metrics from those operations.
- [x] Verify objectives with an independent tiny exhaustive oracle and fixtures where weight changes and batching alter actual order. Add exhausted-budget and asymmetric validation regressions.
- [x] Expose the same request/options and schedule through Node and combined/two-d WASM; add matching declarations and runtime/type smoke checks.
- [x] Add runnable Rust and JavaScript examples, migration/physical assumptions, before/after results and future material-budgeted packing design.
- [x] Build bindings, measure native and browser WASM workloads, request an independent code review, address findings, and run `./scripts/check-quality.sh`.

## Physical rules

Table saw assumes a rip fence for original-sheet vertical cuts and a crosscut sled with an independent stop for horizontal cuts. Panel saw assumes a configurable panel stop. Both allow in-plane quarter turns, recorded explicitly; placement/grain orientation stays fixed. No CNC or stacked cuts are accepted here.

A setup state is the active accessory plus its retained distance. First use of each accessory is an initial setup. Reactivating an accessory or changing its distance is a subsequent setup change. Distances measure the retained reference-side material to the nearest blade edge. State survives loads and sheet boundaries. Each workpiece selection is one handling operation; outputs are put aside as part of the pass, then reloaded explicitly if cut again.

Search considers only full-span cuts flush to a finished boundary, with the blade outside every required part, and both reference edges. This is a finite candidate model, not all conceivable shop strategies. Only report optimality within that model if the entire search is exhausted without pruning or budget truncation. Default results remain heuristic.

Factory edges need no pass. Edge relief permits the blade to exit the current stock, never a finished part to extend beyond it. Physically invalid legacy overhang layouts fail explicitly. Larger-than-kerf gaps can leave actual offcuts; exactly-one-kerf gaps never become extra phantom cuts.
