# Keeping documentation current

Documentation is part of the release gate. Update sources and lockfiles in the
original checkout, then refresh generated facts:

```sh
python3 -B scripts/check-docs.py --write
./scripts/check-quality.sh
```

`--write` updates the [generated reference](reference.md), the README dependency
version, current release-note links and version-pinned GitHub documentation links
in both npm READMEs, plus the WASM import table. It reads Cargo/npm metadata and
TypeScript declarations, and runs `doc_reference` to serialize actual Rust
defaults. It never updates sources in the disposable Scratch mirror. The normal
quality gate regenerates in memory and fails on any difference; CI uses the same
gate before publication.

## Source and verification for changing facts

| Fact | Source / update path | Publication check |
| --- | --- | --- |
| Package version and current release links | Root `Cargo.toml`; synchronize npm manifests and local lock entries, then `check-docs.py --write` | Existing version tests and generated-doc comparison |
| MSRV, pinned toolchain, Node engines, native targets, CI tool versions | Cargo/toolchain/npm manifests and workflow environment | Generated reference comparison |
| Features, WASM imports, API availability | Cargo features, npm exports and `.d.ts` declarations | Generated reference plus packed runtime/type contracts |
| Listed option defaults and preset costs | Rust `Default` implementations and resolved public cut plans | `doc_reference` JSON compared with generated reference |
| Complete Rust examples | Fenced Rust blocks with a `fn main` in maintained guides | Compiled and executed by `check-docs.py --rust-examples`; crate rustdoc examples are compiled by `cargo test --doc`, both in the quality gate |
| Marked JavaScript examples | `<!-- bin-packing:example -->` followed by a complete `js` block | `docs-examples.cjs` runs against native and WASM before publish |
| Local pages and section links | Inline/reference Markdown links | Missing files and Markdown anchors fail the quality gate |
| Model bounds, algorithms, physical/proof semantics | Owning Rust implementation and independent regression tests | Update the matching guide and review it with each semantic source change |
| Test totals, coverage, payload sizes and timings | Fresh command output and measurements | Put them in dated reports with commands, scope and source revision; never label an older measurement as current |

Generated facts describe configuration and declarations. They do not establish
runtime correctness or cross-platform support on their own. Partial snippets
without `fn main` and unmarked JavaScript fragments remain illustrative, so public
examples should prefer complete runnable blocks. Add marked JavaScript examples
to the file list in `bindings/test/docs-examples.cjs` when introducing another
guide. GitHub links to this repository's `main` or the current version tag are
mapped to local source files and anchors, including npm-facing documentation.
Other external links are reviewed separately; the check does not fetch the internet.
Machine-local absolute links are rejected even if they exist on the current host.
Record external evidence paths as inline code so verification does not depend on
one machine's retained artifacts.

Model bounds, source-model field defaults, seed fallback constants and algorithm
explanations require a source review; they cannot be
inferred safely from a version or a passing example. For each changed public
model or solver, review its family guide, README summaries, bindings, defaults,
search limits and proof claims. Use [the release runbook](releasing.md) to record
which gates actually ran. An unexplained documentation check failure blocks
publication just like a test failure.

## Historical documents

Files in `experiments/`, dated files in `superpowers/plans/`, and older release
notes preserve the measurements and decisions of their named snapshots. Do not
rewrite their versions or counts to match a later release. Correct factual errors
with an explicit note, and link from current guides to the appropriate dated
evidence. Current navigation, commands and generated facts live outside those
historical records.
