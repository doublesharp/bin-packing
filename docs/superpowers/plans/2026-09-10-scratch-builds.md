# Scratch build storage plan

Goal: keep this checkout's disposable builds off the source APFS volume, with portable defaults for CI and other machines.

- Add an opt-in setup and command runner using a checkout-specific directory on a mounted Scratch volume. Refuse conflicting configuration, serialize workspace synchronization and commands, and fail if the volume is absent.
- Set an ignored local Cargo target directory and compiler temporary directory. Disable the inherited shared compiler cache for this checkout so it does not write outside Scratch.
- Synchronize tracked and non-ignored source files into a disposable workspace for binding outputs, dependencies, caches, and temporary consumer projects. Keep source and lockfile changes authoritative in the original checkout. Remove stale mirrored source files without deleting build outputs.
- Route the existing quality, feature, and binding checks through the runner when configured. Keep their behavior unchanged when configuration is absent.
- Test synchronization, stale source removal, cache paths, absent-volume handling, argument preservation, setup conflicts, and child exit codes. Configure the real volume, then run the full quality gate and native/WASM package checks there.
