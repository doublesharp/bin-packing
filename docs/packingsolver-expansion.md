# PackingSolver reference expansion

The target is broader native Rust packing functionality and measured improvements
against PackingSolver, while keeping existing Rust, Node and WebAssembly callers
compatible. PackingSolver is a reference and comparison engine, not a mandatory
C++ runtime dependency. Reference revision:
`376b8d1cc4aed5b171bfe1219c037c661faaa7cf`.

## First implementation

- 1D: cost-first multi-stock integer search, preserving count-first APIs and
  supporting kerf, trim, finite inventory and fractional material prices.
- Add independent constrained 2D and 3D APIs. Existing problem/option structs and
  solver ranking retain their meaning.
- 2D: per-sheet border trims and rectangular defects, stock caps, kerf, rotation
  permissions, and count-first or cost-first optimization.
- 3D: per-demand weights, per-bin maximum payloads, rotation masks, stock caps,
  and count-first or cost-first optimization.
- Return source stock/demand indices, deterministic bounded-search diagnostics,
  and independently checkable layouts. Reject malformed constraints rather than
  silently ignoring them. Budget exhaustion may return a valid partial packing;
  an incomplete search never certifies global optimality.
- Add mathematically justified fast paths or lower bounds where they preserve
  the selected complete objective. Do not bypass reusable-offcut ranking without
  proving that its tiebreakers are preserved.
- Make new constrained solvers usable from native Node and WebAssembly.

## Acceptance

Run focused behavioral regressions before implementation, then the required
`./scripts/check-quality.sh`, feature checks and rebuilt binding consumers.
Validate independent geometry, exact demand accounting, weights, trims, defects,
kerf, inventory and source-index provenance, including repeated labels and large
integer values.

Benchmark release builds against the pinned actual PackingSolver executable on
identical normalized inputs. Separate process overhead from solver time. Use
fixed seeds, serial runs, warmups, repeated samples, and matching constraints and
objectives. Validate both engines' output certificates. Report losses and the
scope of every result; do not infer universal superiority from targeted cases.
Store raw source, logs and measurements under the task's Scratch paths.

The implemented batch and its qualified results are recorded in the
[comparison report](experiments/2026-10-28-packingsolver-comparison.md).

## Completion implementation

The remaining groups now have separate native implementations:
[irregular nesting](irregular.md), [stack loading](three-d/loading.md),
[explicit objectives](objectives.md), and [staged cut trees](two-d/staged.md).
The [completion plan](packingsolver-completion-plan.md) records their acceptance
scope, including the dense rectangular repair and broader public comparisons.
The first comparison report above is historical evidence for the initial batch.
The [completion report](experiments/2026-10-28-packingsolver-completion.md)
records the final validation and all three reference suites.

Model bounds, conservative curved geometry and sampled/bounded search are part
of the APIs. Measured results on qualified workloads do not imply a universal
PackingSolver performance lead or equality with every reference model option.
