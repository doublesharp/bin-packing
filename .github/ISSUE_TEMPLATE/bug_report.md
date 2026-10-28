---
name: Bug report
about: Report an incorrect result, regression, build failure, or binding problem.
title: ""
---

<!-- For suspected vulnerabilities, follow SECURITY.md instead of posting details here:
https://github.com/doublesharp/bin-packing/blob/main/SECURITY.md -->

## Problem

Describe the bug. For solver results, identify the violated constraint or the
better known layout. Include whether the solution reported `exact` (or, for saw
plans, its `optimality`).

## Package and environment

- Package: `bin-packing`, `@0xdoublesharp/bin-packing`, or `@0xdoublesharp/bin-packing-wasm`
- Version or commit:
- Operating system and architecture:
- Rust, Node.js, or browser version:
- Feature flags or WASM entry point, if relevant:

## Minimal reproduction

Include runnable code or the smallest problem and options that reproduce the
issue. Specify the algorithm, seed, and any search limits. Remove private data.

```text
Problem, options, and steps to reproduce
```

## Expected behavior

Describe the expected result and why it satisfies the problem's constraints.

## Actual behavior

Include the relevant output or error. For performance regressions, include the
workload, comparison version, and measurement method. Keep large captures out of
the repository and link a sanitized example if needed.
