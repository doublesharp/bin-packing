# Security policy

This policy covers the `bin-packing` Rust crate, the
`@0xdoublesharp/bin-packing` native Node.js package, the
`@0xdoublesharp/bin-packing-wasm` WebAssembly package, and this repository's build
and release tooling.

## Supported versions

Security fixes target the latest published release of each package. Older
releases are not maintained with security backports; upgrade to the latest
release before checking whether an issue still occurs. If upgrading is not
possible, include that constraint in your report.

## Reporting a vulnerability

Email [security@doublesharp.com](mailto:security@doublesharp.com) with the subject
`[bin-packing security]` and a short description. Do not disclose suspected
vulnerabilities through public issues, pull requests, or discussion threads.

Include what you can safely share:

- The affected package, version or commit, platform, runtime, and feature flags
  or WASM entry point.
- A minimal reproduction, including problem input, options, algorithm, seed,
  and search limits where relevant.
- The expected and actual behavior, impact, and conditions needed to trigger
  the issue.
- Any suspected cause, workaround, or proposed fix.

Remove credentials and private customer data from examples. Send a small
reproduction rather than a large output capture. A report does not need a
complete fix to be useful.

## What happens next

Maintainers will review the report, request any information needed to reproduce
it, and coordinate a fix and disclosure with the reporter. Response and fix
timing depend on severity, reproducibility, and maintainer availability; there
is no guaranteed response window. If you have not received a reply, follow up
on the same email thread.

Please allow time to investigate and prepare a fix before publishing exploit
details. Include your preferred attribution, or say if you wish to remain
anonymous.

## Solver bugs and resource use

Incorrect layouts, suboptimal heuristic results, and ordinary build or API bugs
can be reported with the public bug template. Use the private address when an
issue may expose confidential data, compromise a package or release, or allow
an attacker to disrupt an application processing untrusted input. If you are
unsure, report privately first.

Packing and optimization can be expensive even for valid inputs. Applications
accepting untrusted problems should bound dimensions, quantities, search
budgets, and request sizes, and enforce runtime limits outside the solver.
Input validation and safe Rust do not provide an application-level resource
quota.
