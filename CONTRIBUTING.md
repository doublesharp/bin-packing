# Contributing

Contributions to the Rust solvers, Node.js and WebAssembly bindings, tests, and
documentation are welcome. Read [AGENTS.md](AGENTS.md) for the workspace's code
rules and follow the [code of conduct](CODE_OF_CONDUCT.md) in project spaces.

## Before you start

Search existing [issues](https://github.com/doublesharp/bin-packing/issues) and
pull requests before opening a new one. For a new algorithm, public API change,
or substantial performance change, open an issue first to describe the use case,
constraints, and proposed approach. Small fixes and documentation corrections
can go straight to a pull request.

Report suspected vulnerabilities privately using the
[security policy](SECURITY.md). Keep exploit details out of public issues and
pull requests.

## Development setup

Fork the repository and create a branch for one logical change. Run commands
below from the repository root.

Use the Rust toolchain pinned in [rust-toolchain.toml](rust-toolchain.toml),
including Clippy and rustfmt. The minimum supported Rust version is declared in
the root [Cargo.toml](Cargo.toml). Have `python3`, Node.js, `taplo`, and `typos`
available on your `PATH` for the quality gate. Binding development also needs
the tools described in the [Node.js](bindings/node/README.md) and
[WebAssembly](bindings/wasm/README.md) guides; CI's runtime versions and checks
are recorded in [the workflow](.github/workflows/ci.yml).

Disposable build storage is optional. Without it, Cargo and the check scripts
use the normal workspace, as in CI. To keep build output off a backed-up
volume, point setup at a directory on a volume excluded from your backups:

```sh
python3 scripts/scratch.py setup --directory /Volumes/Scratch/bin-packing/cargo
python3 scripts/scratch.py status
```

Once a checkout has `.scratch.json`, commands reuse its saved location and fail
rather than fall back if the volume is unmounted; remount it and retry. See
[Scratch build storage](docs/scratch-builds.md) for configuration and command
routing. Edit sources and lockfiles only in the original checkout.

## Making changes

- Keep changes focused. Separate correctness fixes, API changes, and publishing
  metadata changes into logical commits.
- For behavior changes, add a focused regression test that demonstrates the
  failure or new behavior. Documentation-only changes do not need new tests.
- Preserve the safe-code boundary and structured validation errors. Widen or
  saturate dimension arithmetic, and handle fallible results explicitly.
- Document public items and non-obvious algorithmic complexity. Put shared
  helpers in the relevant model module rather than copying them between
  algorithms.
- Keep the 1D, 2D, and 3D APIs consistent. For public model or serialization
  changes, check both language bindings, their declarations, examples, and docs.
- For randomized behavior, include the problem, options, and seed in regression
  cases. For performance changes, record the workload and measurement method;
  distinguish packing quality from runtime and memory use.

Keep raw logs, screenshots, benchmark output, package archives, and other run
evidence outside Git. On Scratch, use `/Volumes/Scratch/bin-packing/reports/`
or `/Volumes/Scratch/bin-packing/data/`. Commit only short written summaries
when they help explain a change. Clean up task-owned disposable output after
checking ownership and active processes; preserve shared caches.

## Verification

Before submitting a pull request, run the required local gate:

```sh
./scripts/check-quality.sh
```

It runs the runner tests, Rust formatting, TOML formatting, spelling checks,
workspace Clippy with warnings denied, workspace tests, and Rust documentation
generation. With Scratch configured, the script uses the saved build workspace
automatically.

Run additional checks for the affected boundary:

- Feature flags, dependencies, or fuzz targets: `./scripts/check-features.sh`.
- Native or WebAssembly bindings, package exports, or declarations:
  `./scripts/check-bindings.sh`.
- Algorithms: relevant solver tests and, for performance claims, repeatable
  benchmarks against the same inputs and options.

See [workspace verification](docs/workspace-verification.md) for prerequisites,
feature coverage, package checks, and browser checks. Use
`python3 scripts/scratch.py run <command>` for other disposable work. Report
checks you could not run and why; a Rust check alone does not verify a native
package or browser entry point.

## Reporting bugs and proposing features

Use the issue templates. A useful solver report includes the affected package
and version, a minimal problem and options, the algorithm and seed, expected
behavior, actual output or error, and the environment. Remove private customer
data before sharing inputs.

Most algorithms are heuristics. A suboptimal result is useful feedback, but
explain the better known layout and whether the solution reported `exact` (or,
for saw plans, its `optimality`).
For invalid layouts, identify the violated constraint, such as overlap,
inventory, rotation, kerf, or demand accounting.

## Pull requests

Describe the problem and resulting behavior, link any related issue, and list
the checks you ran. Explain compatibility changes and include documentation or
binding updates where needed. Use the pull request checklist to record any
remaining verification limits. Maintainers may request changes before merging.

## License

Contributions are provided under the project's [MIT License](LICENSE). Only
submit work you have the right to contribute, and retain required attribution
for third-party material.
