# Terra Low test batches

The subsequent [existing-test branch checks](2026-10-28-branch-proof.md) record
further case-table amendments and controlled fault evidence.

Six `gpt-6.1-terra` workers at low reasoning effort added 25 Rust tests in two
small batches. Each worker owned one file and added four or five tests. Root
executed and reviewed all returned cases; all 25 pass.

The additional contracts cover:

- Constrained 2D: forged source quantities, identical-stock inventory, diagnostic
  and material totals, exact trim/defect boundaries.
- Constrained 3D: inventory, proof/diagnostic arrays, forbidden rotations with
  identical extents, unplaced source order and coordinate overflow.
- Offcut reuse: option/forecast ceilings, source error indices, zero-weight
  accounting, extractable geometry and kerf/rotation provenance.
- Irregular nesting: aggregate quantity/open-objective limits, invalid defects,
  reflection in orientation caps and partial-stock source accounting.
- Staged cutting: machine/control boundaries, group shape, finite-stock partial
  results and first-axis provenance even for an uncut sheet.
- Pattern recombination: demand/stock type fallbacks, finite inventory, and the
  inclusive 128-piece and 32-control limits.

Worker edits were confined to test code. Production code across 63 source files
matches the previous verified package after excluding test-only items. That
package snapshot was first checked against the preceding coverage source hashes.
No library defect was established by these batches.

## Verification

The required `./scripts/check-quality.sh` passes **602 Rust tests** and **43
Python tests**, formatting, metadata/text checks, strict workspace Clippy and
rustdoc. One pre-existing optional benchmark remains ignored.

Production Rust coverage is **96.22% of lines** (15,254/15,853) and **84.47% of
branch outcomes** (3,166/3,748). The all-feature instrumented run also passes all
602 tests. Production source hashes remain unchanged during measurement.
The measured results meet the testing guide's minimums of 96% lines and 84%
branches.

The scope and counting method match the preceding measurement: unique LLVM LCOV
source lines and branch outcomes, excluding test code. JavaScript/browser line
coverage is outside this percentage. See [the testing guide](../testing-confidence.md)
for the repeatable command and scope.

Initial parallel worker test commands encountered a shared Scratch lock held by
suspended compiler processes. Subsequent workers returned small source batches;
root ran their tests centrally. All unexecuted worker cases were run before being
counted as passing. Only verified task-owned suspended processes were resumed.

Raw focused/gate logs, profiles, missing-line/branch lists, source comparison and
measurement receipts remain under
`/Volumes/Scratch/bin-packing/reports/terra-batches-20261028/`. Disposable coverage
build output was removed after ownership/open-file checks. Shared warm artifacts
and the preceding receipts remain intact.
