# Disposable builds on Scratch

This checkout can use a mounted APFS Scratch volume for build outputs, temporary files, dependency downloads, and binding package assembly. Source code, lockfiles, fixtures, and committed benchmark evidence stay in the source checkout.

On this Mac, `/Volumes/Scratch` is excluded from Time Machine. Its APFS snapshots are separate from snapshots of the source volume. Setup does not change volume, backup, or system-wide tool settings. Check the volume with `tmutil isexcluded /Volumes/Scratch` and `diskutil apfs listSnapshots /Volumes/Scratch`.

## Setup

```sh
python3 scripts/scratch.py setup
python3 scripts/scratch.py status
```

The setup writes ignored `.scratch.json` and `.cargo/config.toml` files. Each checkout gets its own directory under `/Volumes/Scratch/bin-packing/`, based on its absolute path. Existing Cargo configuration is preserved by refusing to overwrite it. Setup can be rerun after disposable data is cleared.

Ordinary Cargo commands started in this checkout use the Scratch target directory and compiler temporary directory. The local configuration disables the inherited compiler cache wrapper, avoiding writes to a shared cache on the source volume. Explicit Cargo environment overrides still take precedence over configuration.

## Build and verify

The three check scripts automatically use Scratch after setup:

```sh
./scripts/check-quality.sh
./scripts/check-features.sh
./scripts/check-bindings.sh
```

Run other build commands through the wrapper:

```sh
python3 scripts/scratch.py run --cwd bindings/node pnpm install --frozen-lockfile
python3 scripts/scratch.py run npm --prefix bindings/node run build
python3 scripts/scratch.py run npm --prefix bindings/wasm run build
python3 scripts/scratch.py run cargo run -p bin-packing --example saw_planner
python3 scripts/scratch.py run --cwd bindings/node npm pack --ignore-scripts
python3 scripts/scratch.py run sh -c 'cargo test > "$TMPDIR/tests.log" 2>&1'
```

Shell redirection outside the wrapper happens in the calling shell. Put disposable log redirection inside `sh -c`, as above, or use an explicit Scratch destination.

The wrapper copies tracked and non-ignored files, including uncommitted edits, to `<scratch-root>/workspace`. It removes stale copied source files while retaining generated outputs. Commands are serialized per checkout so another invocation cannot change copied sources during a build. Command arguments and exit statuses are preserved.

The runner sets Cargo's target and download directories, npm, pnpm, Corepack and wasm-pack caches, `XDG_CACHE_HOME`, and temporary directories inside Scratch. Installed dependencies, native binaries, WASM `dist`, package tarballs, and test output stay in the disposable workspace. This avoids symlink-related omissions when packing npm packages.

Edits made by commands inside the disposable workspace do not return to the source checkout. Use the original checkout for source edits, dependency/version changes, and Git commands. Direct npm commands outside the wrapper still use the source directory; use the wrapper to keep their outputs on Scratch. Without local setup, CI and other contributors keep the normal behavior.

If Scratch is unmounted, the runner fails rather than falling back to the source volume. Remount the volume and retry. If its contents were deleted, rerun setup and reinstall dependencies. Global Rust toolchains and caches used by other projects are not moved.

## Runner tests

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests
```
