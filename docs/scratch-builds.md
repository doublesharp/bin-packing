# Disposable builds on Scratch

This checkout can use a chosen build directory on macOS or Linux for build outputs, temporary files, dependency downloads, and binding package assembly. Source code, lockfiles and test fixtures stay in the source checkout. Raw run evidence stays outside git under `/Volumes/Scratch/bin-packing/reports/` or `/Volumes/Scratch/bin-packing/data/`; keep only short human-written summaries in docs.

Choose a directory on a volume that your backup and snapshot policy excludes. Setup does not change volume, backup, or system-wide tool settings. On macOS, inspect the chosen volume with `tmutil isexcluded <volume>` and `diskutil apfs listSnapshots <volume>`.

## Setup

```sh
python3 scripts/scratch.py setup --directory /Volumes/Scratch/bin-packing/cargo
python3 scripts/scratch.py status
```

The setup writes ignored `.scratch.json` and `.cargo/config.toml` files. Each checkout gets its own `bin-packing/<checkout-id>` directory under the selected path. There is no system-specific default. The directory and its containing mount are saved, so normal commands need no path arguments or environment variables.

Use `--volume /path/to/mounted-volume` instead when you want setup to require an exact mount point. Run `setup` without arguments to reuse the saved location, including after disposable data is cleared. Run it with a new `--directory` or `--volume` to change locations. Old build data is preserved for later cleanup. Only the tool's own generated Cargo configuration is replaced; unrelated or modified Cargo configuration is preserved by refusing the change.

Ordinary Cargo commands started in this checkout use the Scratch target directory and compiler temporary directory. The local configuration disables the inherited compiler cache wrapper, avoiding writes to a shared cache on the source volume. Explicit Cargo environment overrides still take precedence over configuration.

## Build and verify

Cargo, the three check scripts, and npm build/test/clean scripts automatically use the saved storage after setup:

```sh
./scripts/check-quality.sh
./scripts/check-features.sh
./scripts/check-bindings.sh
npm --prefix bindings/node run build
npm --prefix bindings/node test
npm --prefix bindings/wasm run build
npm --prefix bindings/wasm test
```

The wrapper remains available for ad hoc commands and package installation or assembly:

```sh
python3 scripts/scratch.py run --cwd bindings/node pnpm install --frozen-lockfile
python3 scripts/scratch.py run cargo run -p bin-packing --example saw_planner
python3 scripts/scratch.py run --cwd bindings/node npm pack --ignore-scripts
python3 scripts/scratch.py run sh -c 'cargo test > "$TMPDIR/tests.log" 2>&1'
```

Shell redirection outside the wrapper happens in the calling shell. Put disposable log redirection inside `sh -c`, as above, or use an explicit Scratch destination.

The wrapper copies tracked and non-ignored files, including uncommitted edits, to `<scratch-root>/workspace`. It removes stale copied source files while retaining generated outputs. Commands are serialized per checkout so another invocation cannot change copied sources during a build. Command arguments and exit statuses are preserved.

The runner sets Cargo's target and download directories, npm, pnpm, Corepack and wasm-pack caches, `XDG_CACHE_HOME`, and temporary directories inside Scratch. Installed dependencies, native binaries, WASM `dist`, package tarballs, and test output stay in the disposable workspace. This avoids symlink-related omissions when packing npm packages.

Edits made by commands inside the disposable workspace do not return to the source checkout. Use the original checkout for source edits, dependency/version changes, and Git commands. Package scripts for builds, tests, and cleaning load the saved setting automatically. Dependency installation, raw tool invocations, and `npm pack` are separate operations; use the wrapper for their disposable outputs. Package publishing hooks continue to operate in their current package directory. Without local setup, CI and other contributors keep the normal behavior.

If Scratch is unmounted, the runner fails rather than falling back to the source volume. Remount the volume and retry. If its contents were deleted, rerun setup and reinstall dependencies. Global Rust toolchains and caches used by other projects are not moved.

## Runner tests

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests
```
