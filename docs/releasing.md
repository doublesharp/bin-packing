# Release runbook

The release workflow publishes the Rust crate to crates.io, native platform and
wrapper packages to npm, the WASM package to npm, and a GitHub release. This
repository has no separate hosted application deployment. docs.rs builds the
published Rust API documentation independently after crates.io ingestion.

## Prepare one version

1. Check the latest Git tags, GitHub releases and crates.io/npm versions. Use one
   version across the workspace and both npm packages. Do not reuse an already
   published version or advance a tag after publishing begins.
2. Update `[workspace.package].version`, both binding `package.json` versions,
   local workspace entries in `Cargo.lock`, and the core entry in `fuzz/Cargo.lock`.
   Preserve third-party versions. Binding Rust crates inherit the workspace
   version and remain unpublished implementation crates.
3. Write `docs/releases/vVERSION.md` with changes, compatibility, model/search
   limits and actual verification. Older release notes are historical records.
4. Review every maintained guide affected by source changes. Refresh generated
   facts with `python3 -B scripts/check-docs.py --write`. Review its diff, update
   runnable examples and resolve source/doc disagreements. See
   [documentation maintenance](documentation.md).

## Verify before tagging

Use the Scratch wrapper for all disposable work when local storage is configured:

```sh
./scripts/check-quality.sh
./scripts/check-features.sh
./scripts/check-bindings.sh
python3 scripts/scratch.py run cargo package -p bin-packing --locked
git diff --check
```

The package command verifies the actual crate archive without publishing. Binding
checks rebuild native and all configured WASM variants, verify tarballs,
declarations, documented examples, generated cases and actual Chromium modules.
Install the TypeScript/Playwright tools described in
[workspace verification](workspace-verification.md). Record platform scope and any
unrun gate. Commit source/docs/lockfile changes; leave all raw evidence on Scratch.

## Publish the checked commit

Push the commit to `main` and confirm its CI result. Create the stable tag
`vVERSION` on that exact commit and push it. A tag push runs the same quality and
package checks plus native release builds for the targets declared in the
[generated reference](reference.md). The publish job depends on all of them.
Both ordinary and tag CI run native/WASM documentation examples, declarations,
generated solver cases and actual Chromium contracts. Generated docs, local links
and complete Rust examples run in the quality job.

The workflow verifies that the stable tag equals Cargo and both npm versions,
prepares the exact-version native optional dependencies, checks tarballs, uses
`cargo publish --dry-run`, then publishes packages and creates the GitHub release
using this version's release notes. npm uses configured trusted publishing;
crates.io uses the repository's configured token. Never print credentials or
move publication to an unrelated credential context to bypass a failed gate.

For a failed existing-tag release, inspect the precise failed step and registries
first. `workflow_dispatch` accepts an existing stable tag, but the publish steps
are not generally idempotent: a package already published at that version can
make a blind rerun fail. Recover only the missing operations, with the same
verified artifacts and version. Fix a defective published artifact in a new
version; do not silently substitute different binaries under the same version.

## Read back the result

Confirm the release workflow succeeded and the GitHub tag resolves to the checked
commit. Inspect crates.io and both npm registries for the intended versions and
native optional dependency map. Load isolated downloaded packages and check
`version()` and representative documented functions. Confirm the versioned
docs.rs build is available; a queued or failed docs.rs build remains a separate
open documentation gate even if package publication succeeded.

Retain commands and raw results outside Git. Clean task-owned temporary package
projects/downloads after checking ownership and open processes, preserving the
checkout's warm shared caches. A successful local test or an accepted registry
upload alone does not establish every release outcome above.
