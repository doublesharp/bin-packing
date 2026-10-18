# Packing experiments implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. The user approved implementation, testing, common-branch integration, and cleanup of these experiments on 2026-10-14. Execute continuously.

**Goal:** Implement and evaluate all seven proposed solver improvements, retain useful results, and combine them on `feature/packing-experiments` for testing.

**Architecture:** Each idea has a managed worktree and a bounded experiment. Expensive new searches are explicit Rust entry points so existing default solves remain stable. Transparent pruning changes may improve existing implementations when independently validated. Retain original incumbents and compare the complete existing ranking contract before replacing them.

**Tech Stack:** Rust 2024, Rust 1.99, existing dependencies, serde, seeded searches, Criterion or a deterministic JSON example for paired measurements.

## Global constraints

- `#![forbid(unsafe_code)]` stays on. No unsafe blocks, unwrap, todo, dbg, or library expect/panic.
- Public boundaries validate input. Preserve structured errors, public docs, deterministic seeds, kerf, trim, rotation, edge relief, and inventory constraints.
- Dimension arithmetic widens before multiplication or uses saturation. No silent fallible-result discards.
- No unsupported optimality claims. A restricted pattern pool or timed-out heuristic cannot certify the unrestricted optimum.
- Preserve input demand identity even when names repeat. Validate every reported layout independently against counts, orientation, bounds, and overlap.
- New expensive objectives or search families are opt-in. No external solver dependency is required for these bounded experiments.
- Each idea starts at baseline `0b626ac920d2ef5845c5e1763a2999ee5baf0c29` in its own `feature/packing-*` branch and managed worktree.
- Tests first, observe the intended failure, implement, rerun focused checks, then commit only deliberate changes.
- Scratch build outputs and diagnostic logs are disposable. Preserve reproducible committed fixtures and results. Do not clean unrelated worktrees or caches.
- Required final gate: `./scripts/check-quality.sh`; additionally test relevant feature combinations and existing bindings for API/schema regressions.

## Task 1: Cross-solver pattern recombination

Worktree: packing-pattern-pool. Files: new `two_d/pattern_pool.rs`, `two_d/mod.rs`, targeted integration tests.

- [x] Add a failing fixture where pooling valid sheet patterns permits a better combination than selecting whole candidates; test repeated demand names and capped stocks.
- [x] Expose documented `solve_2d_recombined(problem: TwoDProblem, options: TwoDOptions) -> Result<TwoDSolution>`.
- [x] Tag demands by source index before constructing candidates, collect bounded complete physical sheet patterns, select exact demand counts with memoized bounded integer search and inventory caps, restore original names.
- [x] Preserve the best complete candidate on cap exhaustion or no improvement; never assert global optimality from the restricted pool.
- [x] Verify no regression under the ordinary solution comparator and independent geometric checks; record time, states and improvements.

## Task 2: Joint target-bin repair

Worktree: packing-joint-repair. Files: `three_d/local_search.rs`, optional focused repair module and public wrapper, regression tests.

- [x] Prove with a failing fixture that a bin can be eliminated only after neutral rearrangement or joint destination repacking.
- [x] Implement bounded seeded target-bin destroy-and-repair, including repacking destination contents rather than freezing all placements. Keep the true incumbent separately from exploratory state.
- [x] Use a documented opt-in `solve_3d_repaired(problem: ThreeDProblem, options: ThreeDOptions) -> Result<ThreeDSolution>` unless a default change is justified by measurements. Preserve all constraints and duplicate-name identity.
- [x] Verify meaningful diversification, non-regression, bounded work, and independent geometry; report the currently ineffective rotate/swap/restart behavior separately.

## Task 3: Demand-aware spaces and mixed rotation counts

Worktree: packing-demand-space. Files: MaxRects helper and rotation search, `two_d/mod.rs`, targeted tests.

- [x] Add adversarial fixtures where preserving a remaining part's last viable cavity matters and where mixed rotation counts improve a uniform assignment.
- [x] Expose explicit `solve_2d_demand_aware` and `solve_2d_mixed_rotations` entry points with ordinary `(problem, options)` signatures.
- [x] Rank a bounded shortlist by remaining-demand opportunity loss with overlap-aware estimates; search rotation counts per repeated demand type and seeded orders without assuming placement completeness.
- [x] Keep baseline incumbents, validate geometry/kerf/inventory, and measure added work versus quality gains.

## Task 4: Stronger geometric lower bounds

Worktree: packing-shape-bounds. Files: new `three_d/bounds.rs`, `three_d/exact.rs`, module declaration, tests.

- [x] Add the nine side-40 cubes in a side-100 bin fixture: the lower bound must be two, not one.
- [x] Implement integer conservative-scale/dual-feasible bounds for the existing supported exact backend. Check each bound against brute-force tiny instances and orientation preconditions.
- [x] Use valid bounds for pruning and count-optimality reporting; preserve existing correctness on incomplete searches.
- [x] Measure expanded nodes and elapsed time on structured and mixed cases.

## Task 5: Canonical saw states

Worktree: packing-saw-states. Files: `two_d/saw_plan/search.rs`, targeted tests.

- [x] Add a repeated-sheet fixture exposing equivalent future physical states reached through commuting cuts.
- [x] Deduplicate using the complete future-relevant state: ready geometry and part identity, orientation, sheet-order constraints, active setup, and used accessories. Keep nondominated count vectors and one replayable trace.
- [x] Bound table memory. Preserve deterministic ties and report search truncation honestly; do not merge states merely because dimensions match.
- [x] Verify full physical replay and objective non-regression, and measure duplicate counts, effort, memory and elapsed time.

## Task 6: Future-use offcut valuation

Worktree: packing-offcut-value. Files: new two_d reuse module, public re-exports/wrapper, targeted tests.

- [x] Add a fixture where a smaller but appropriately shaped physical offcut serves forecast demand better than a larger offcut.
- [x] Add validated serde-friendly forecast types and an explicit solve/evaluation API. Forecast weights are finite and nonnegative; dimensions are validated.
- [x] Score actual reusable, physically recoverable offcuts against forecast shapes. Keep hard inventory and explicit material limits at the baseline's count/cost by default. Ordinary solve ranking remains unchanged.
- [x] Test zero/invalid forecasts, rotation, kerf, non-overlapping recoverable remnants, and material-budget rejection. Measure forecast fulfillment and disclose dependence on the supplied forecast.

## Task 7: Compressed multi-stock 1D pattern search

Worktree: packing-arc-flow. Files: new `one_d/arc_flow.rs`, `one_d/mod.rs`, targeted tests.

- [x] Add a fixture with several stock lengths/inventory limits where the current projected greedy incumbent is improvable, plus a large repeated-demand compression fixture.
- [x] Expose documented `solve_1d_arc_flow(problem: OneDProblem, options: OneDOptions) -> Result<OneDSolution>`.
- [x] Implement a bounded compressed pattern DAG with per-stock kerf/trim and an exact integer selection search over demand counts and availability. Do not claim polynomial min-cost flow solves the constrained integer problem.
- [x] Fall back to the incumbent on budget exhaustion; report graph/search work and exactness only when complete for the supported domain.
- [x] Test independent bar accounting, procurement estimates, mixed stock/cost/caps and cap exhaustion. Measure memory, patterns represented, quality and elapsed time.

## Integration and acceptance

- [x] Independently review each task diff for requirements and quality. Resolve important findings before integration.
- [x] Record accepted, conditional and rejected outcomes with exact commits and commands in `docs/experiments/2026-10-14-packing-findings.md`.
- [x] Merge useful commits into `feature/packing-experiments`; resolve shared module edits explicitly.
- [x] Add a reproducible paired benchmark/example and a JSON result artifact on Scratch. Measure serially after workers finish, with the same inputs, seeds, build and hardware; distinguish per-fixture gains from general performance.
- [x] Run the required full gate and relevant feature/binding checks on the combined branch. Review the whole branch.
- [x] Archive only worktrees created for this experiment after their results/commits are preserved. Keep the common testing branch and its clean worktree available for the user. Do not change main or publish.

Final acceptance: all seven experiments were implemented and evaluated. Expensive searches remain opt-in. Batched saw canonical pruning was rejected after an eightfold slowdown and bypassed in `207135e`; sequential pruning remains. Combined code `1c05c74` passed all required gates and the final paired benchmark. Eleven temporary worktrees are archived; the common testing worktree remains. The short findings summary is in `docs/experiments/2026-10-14-packing-findings.md`; raw run artifacts are preserved outside git on Scratch.
