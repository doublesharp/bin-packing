# Existing-test branch and fault checks

Three Terra Low workers extended five existing Rust tests. The suite still has
602 tests; the added coverage comes from more input cases inside those tests.

The amendments cover six small search budgets on two 2D problems, all four
removed borders with valid counterparts, four exact-fit/inventory scenarios
across 28 3D strategies, and irregular transform/source-accounting cases.
Reflection and rotation permission checks preserve the saved geometry while
changing source permissions, isolating the intended validation rule.

## Controlled fault evidence

Each experiment ran four steps: the expanded test on correct code; the previous
version of that focused test on a deliberately faulty implementation; the
expanded test on the same fault; and the expanded test after exact restoration.
The previous test functions came from the archived, previously verified package
and ran inside the current harness.

| Deliberate production fault | Previous focused test | Expanded focused test | Restored original |
| --- | --- | --- | --- |
| Omit the right trim from the placement bound | Passes, misses fault | Fails at removed-border assertion | Passes |
| Compare a 3D item's height with bin width | Passes, misses fault | Fails at exact-fit placement assertion | Passes |
| Omit reflection permission from readback validation | Passes, misses fault | Fails after permission revocation | Passes |

All three failures were executed test assertions. Compiler failures and timeouts
were not counted. Before/after hashes confirm exact restoration of every touched
source file. Production code across 63 files also matches the previous verified
package after excluding test-only items.

These controls establish detection for the three selected mistakes. They compare
versions of the same focused tests, rather than measuring mutation coverage of
the entire prior suite. Useful new input cases can detect mistakes even when
they follow a branch already visited by older cases.

## Final measurements

Production Rust branch coverage is **84.71%** (3,175/3,748 outcomes), with nine
additional outcomes covered. The previous measurement was 84.47% (3,166/3,748).
Line coverage is **96.25%** (15,258/15,853). The scope, compiler and denominator
match the preceding measurement; no new coverage exclusions were introduced.
There are 573 uncovered branch outcomes, listed by file in the raw summary.

The required `./scripts/check-quality.sh` passes **602 Rust tests**, **43 Python
tests**, formatting, metadata/text checks, strict workspace Clippy and rustdoc.
One pre-existing optional test remains ignored. The all-feature instrumented run
also passes all 602 Rust tests on the restored source. Measured source hashes
remain unchanged during the run.

See [the testing guide](../testing-confidence.md) for the repeatable command and
scope. This percentage covers the main production Rust library; JavaScript and
browser line coverage are separate.

Focused logs, old/expanded function snapshots, source backups, restoration hashes,
profiles, the self-contained comparison chart and final receipts are under
`/Volumes/Scratch/bin-packing/reports/branch-proof-20261028/`. Disposable coverage
build output was removed after ownership/open-file checks. Shared caches remain.
