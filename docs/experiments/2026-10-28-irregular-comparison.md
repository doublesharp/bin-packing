# Native irregular nesting and PackingSolver comparison

The comparison uses the native Rust irregular solver and PackingSolver revision
`376b8d1cc4aed5b171bfe1219c037c661faaa7cf`. The fixed selection contains 14 targeted
feature cases and 10 public fixtures from that revision's
[`data/irregular/tests`](https://github.com/fontanf/packingsolver/tree/376b8d1cc4aed5b171bfe1219c037c661faaa7cf/data/irregular/tests).
It measures this selection and these controls; it cannot establish a general
performance or solution-quality lead.

The native implementation remains entirely Rust. Geometry, approximation,
rotation sampling, inventory, objectives, and search limits are documented in
[irregular.md](../irregular.md).

## Method and matching semantics

`scripts/compare-irregular-packingsolver.py` runs each solver serially with one
untimed warmup and three measured repetitions. The Rust release harness measures
solve calls, including cloning and native validation. PackingSolver reports its
own internal `Time`, using `not-anytime-sequential`, explicit HiGHS, and a
two-second time limit. Rust uses a 32,768-probe construction cap plus at most
4,096 improvement probes, beam width eight, 128 translations per orientation/bin,
15-degree maximum angular sample gaps, at most 64 orientations, and curve
tolerance 0.01. These are different search budgets. Process wall times are
retained separately; time to a common quality threshold is not measured.

Rust warmups and measurements share one harness process. Each reference warmup
and measurement launches a fresh CLI process. Startup is excluded from internal
timings, while initialization performed within a solver's timer remains
included. This lifecycle difference also limits direct performance conclusions.

The public fixtures retain their original geometry, holes, components, legal
angles, reflection permissions, copies, spacing, and objective. Upstream default
cost and profit are analytic material area, while default bin copies equal one;
the adapter makes those defaults explicit. Source indices are preserved. Every
translated instance must roundtrip to exactly the same canonical native problem
and matching controls before the reference runs. Original public-file hashes,
normalized input hashes, source-file hashes, executable hashes, build
configuration, host load, raw commands, logs, outputs, and certificates are kept
under Scratch.

The independent Python validator reconstructs source transforms using analytic
line segments and circular arcs. It divides the plane at boundary extrema and
intersections, then checks every vertical arrangement slab for containment or
material overlap. Analytic primitive distances verify item, bin, and defect
clearances. No raster or inscribed-chord-only check decides placement validity.
The spatial tolerance is the larger of `1e-7` and absolute coordinate scale times
`1e-9`. Certificate contours must remain connected and match the original
transformed components and holes. Both engines' original material area, source
counts, stock inventory, profit, cost, shortfalls, and extents are checked.
PackingSolver summary extrema refer to its last bin group; an empty solution has
null extrema. These cases have dedicated regression tests.

Some available features have different semantics and are excluded from direct
scoring. PackingSolver requires a fixed aspect ratio for open XY, whereas the
native XY objective permits a free aspect ratio. Per-angle mirror-only
permissions differ from native globally optional reflection. Native bin holes
and multiple bin components do not map to the upstream single outer bin reader.
Fixed items, material quality rules, resources, and periodic packing are outside
this matching comparison model.

## Fixed selection

Targeted cases cover L-shape interlocking, item-in-hole placement, rigid multiple
components, circular arcs, circle tangency, mirrored concavity, rotation
intervals, item spacing, defect spacing, total cost, open X, open Y, fractional
contacts, and a curved hole. These are feature regressions rather than held-out
performance samples.

The public fixtures are `circle`, `circular_bin`, `rotations`, `mirror`,
`feasibility_squares_triangles`, `polygon_with_hole`, `bin_packing_squares`,
`knapsack_item_item_minimum_spacing`, `knapsack_item_defect_minimum_spacing`, and
`open_dimension_x_mixed_shapes`. They were selected for matching semantics and
supported cardinality before reference outcomes were inspected. Losses and
partial results remain in the report.

## Final measurements

The frozen-source run on October 28 completed all 24 cases with one warmup and
three measured repetitions per engine. All results passed independent readback;
the report records no failures and confirms that source and executable hashes
remained unchanged. The 96 reference certificates, including warmups, were
validated. Native measured outputs were identical across repetitions and were
independently validated. The implementation has 17 focused Rust regressions and
the comparison tooling has 11 Python regressions.

Quality matched on all 14 targeted cases and nine of the ten public cases.
PackingSolver won `open_dimension_x_mixed_shapes`: both engines placed all 14
items, but native width was 30 versus 24.870129870129865 in every reference
sample, a 20.6% width increase. There were no native quality wins. Nineteen native
results reported truncated search; no result establishes global optimality.

Knapsack equality refers to placed profit, including shortfalls. Both engines
placed two of four items for `circular_bin`, four of nine for
`knapsack_item_item_minimum_spacing`, and zero of two for
`knapsack_item_defect_minimum_spacing`. These remain explicit in the results.

Among the 23 equal-quality cases, native had a lower measured internal median on
eight and a higher median on fifteen. One of the eight was effectively a near
tie: `polygon_with_hole` measured 2.286 versus 2.300 milliseconds. The following
examples all have equal final quality; the quality-loss case is excluded from
timing comparisons.

| Equal-quality case | Native median, ms | PackingSolver median, ms |
| --- | ---: | ---: |
| Targeted rigid components | 1.099 | 4.695 |
| Targeted mirrored concavity | 0.229 | 1.985 |
| Targeted circular arc | 3.260 | 0.244 |
| Targeted curved hole | 257.452 | 103.056 |
| Public circle | 4.119 | 0.196 |
| Public squares and triangles feasibility | 37.586 | 3.605 |

The macOS ARM host has 12 logical CPUs. Its recorded load averages were
`7.77 / 23.56 / 23.68` at the start and `8.53 / 22.94 / 23.46` at the end
(one/five/fifteen minutes). The one-minute value ranged from 7.77 to 8.53; the
longer averages retain substantial earlier load. Combined with unequal effort
budgets, different process lifetimes, and only three measurements, these
conditions do not support a general speed claim or a claim about time to equal
quality.

Raw inputs, commands, outputs, certificates, logs, samples, and full hashes are
in the untracked [local Scratch report](/Volumes/Scratch/bin-packing/reports/irregular-completion-20261028/final.json)
and its adjacent `final-artifacts` directory. The combined source SHA-256 is
`15ecf0b2809b4aaa15a5d15f316707f5835a4f9d9efca36e97ff24d49f06b70c`;
the report SHA-256 is
`84c6d7da6b15240869007e0e78717eea24c1ee4d7a44a79347b7797d63aac1ce`.
