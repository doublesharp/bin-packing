# Combined branch independent review

Reviewed branch: feature/packing-experiments at a918a44.
Baseline: 0b626ac920d2ef5845c5e1763a2999ee5baf0c29.
Review mode: read-only source review; no builds, tests, benchmarks, or worktree mutations performed.

## Result

Spec/quality review passes with no actionable correctness findings in the inspected combined diff. This is a source-review conclusion; the root agent's combined quality gate, feature/binding checks, physical validation and uncontended measurements remain separate evidence.

## Inspected behavior

- Arc search: compressed DAG construction and streamed paths, demand-row identity, stock usage and finite caps, kerf/trim reconstruction, bounded search termination, partial incumbents, memo dominance, floating-point cost restrictions, exactness and procurement-estimate claims.
- Pattern recombination: portfolio parity, whole physical patterns, indexed names, independent geometry checks, finite stock caps, complete-demand selection, memo state and componentwise offcut dominance, fallback/input bounds, incumbent retention and lack of unrestricted optimality claims.
- Joint repair: selected stock multiset, borrowed item pool and reconstructed placements, neutral exploratory states versus strict incumbent improvements, rotation identities, reservation of unplaced records, public procurement summaries and guillotine bypass.
- Demand-space/mixed rotations: shared input boundaries, unique indexed identities, independent result validation, bounded shortlists and non-additive cavity capacities, individual mixed-count rotation assignments, deterministic trial budgets and ordinary incumbent retention.
- Conservative bounds: floor scaling and per-axis interval-precedence compaction argument, dimension/quantity accumulation width, orientation restrictions, and count optimality only when the feasible incumbent reaches the valid lower bound.
- Saw-state pruning: exact geometry and part identities, source stock, sheet-order constraint, orientation, active setup, accessory state, componentwise count dominance, retained trace ancestry, memory-pressure bypass, and heuristic status after beam/budget truncation. New report fields have serde defaults.
- Reuse: source validation before indexing, original stock/part identity restoration, physical terminal offcuts, disjoint greedy subdivision with kerf/edge rules, bounded forecasts and evaluation, source-cost material ceilings, stock caps and explicit forecast-dependent comparator.
- Benchmark harness: paired ordinary inputs, fixed seeds, release builds, serial Rayon configuration, untimed warmup, timed input cloning, percentile/sample recording, pinned baseline, and explicit targeted-fixture scope. No broad performance inference is justified by these fixtures alone.

## Acceptance scope

Retain the five new opt-in solver families and forecast API as bounded experiments, subject to the required final gate and measured findings. Default-path changes are the conservative bound and saw-state deduplication, plus behavior-preserving shared helper refactors. The documentation should preserve the existing limitations: restricted pools and heuristic repairs are not unrestricted optimality proofs; forecast fulfillment is greedy and forecast-dependent; relaxed inventory requirements are estimates; per-fixture performance is not representative production performance.

## Consumer declaration follow-up: 7558dcb

Reviewed commit 7558dcb7c1c7c81278af7899b69cadb454032c22 read-only. The root agent found that the newly serialized saw-search diagnostics were missing from the TypeScript consumer declarations; that gap was not identified in the original source review above.

The fix correctly adds `dominated_states: number`, `peak_dominance_units: number`, and `dominance_table_saturated: boolean` to all three declaration sources: native wrapper, full WASM, and 2D-only WASM. Source search found no additional SawSearchReport2D interface definitions requiring the same change. WASM build configuration copies the full/2D declaration sources into their bundled, web and Node variants.

Regression coverage is appropriate: the shared native/WASM smoke fixture checks both counters are safe nonnegative integers, enforces the 65,536-unit storage ceiling, and checks saturation is boolean. The package-types generator explicitly accesses and returns all three fields with their expected tuple types for every package export containing 2D and for the native declaration. Thus omission or incompatible typing is a compiler error; the runtime checks independently catch missing serialized fields. These checks are wired into the existing binding smoke suites and scripts/check-bindings.sh.

Source-review conclusion: pass; no further actionable findings in this follow-up. The parent reports an observed TS2339 red followed by compile green. I did not rerun compilation or binding tests; the rebuilt binding gate remains separate verification owned by the root agent.

## Measured saw-pruning restriction follow-up

Reviewed the three-file patch in feature/packing-saw-adaptive relative to 7558dcb read-only. The root's paired measurements found an approximately eightfold batched-search slowdown without effort improvement, while sequential search benefited. Restricting the canonical table to sequential search is consistent with the requested keep-what-works acceptance rule.

Source-review conclusion: pass, no actionable findings. `batch_across_sheets = true` now constructs no dominance table, no future-state keys, and performs no dominance lookup or successor filtering. The optional-key representation preserves the original successor order and beam logic on that path. Sequential search retains the previously reviewed key/admission/filter behavior. Diagnostics correctly remain zero/false when the optimization is bypassed, and their Rust documentation states this explicitly.

The integration regression exercises both batching modes, independently replays physical cuts, checks effort against the feasible baseline, asserts deterministic output, verifies that sequential pruning actually runs within its storage cap, and requires all pruning diagnostics to be zero/false for batching. These assertions directly cover the restriction. Focused test/lint success was reported by the parent; I did not run builds, tests or measurements. Root's final combined gate and uncontended before/after timing remain the acceptance evidence for the integrated patch.
