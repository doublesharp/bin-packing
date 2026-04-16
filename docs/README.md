# Algorithm reference

This directory documents every algorithm that `bin-packing` ships, one page per
algorithm family. Each page explains the *mechanism* — how the algorithm
builds a solution, what it ranks candidates by, and why it exists as a
distinct variant — in enough detail to choose intelligently between them and
to debug surprising output.

For a quick feature tour or a Getting Started guide, see the top-level
[README](../README.md). For API reference, use `cargo doc --open` or
[docs.rs/bin-packing](https://docs.rs/bin-packing).

## Layout

- **[one-d/](one-d/)** — one-dimensional cutting stock (linear bar / pipe
  stock). Entry point: `solve_1d`.
- **[two-d/](two-d/)** — two-dimensional rectangular sheet packing. Entry
  point: `solve_2d`.
- **[three-d/](three-d/)** — three-dimensional rectangular bin packing.
  Entry point: `solve_3d`.

## 1D algorithms

| Algorithm | Page | Exact? | Honors `seed` |
| --- | --- | --- | --- |
| First-Fit Decreasing | [one-d/first-fit-decreasing.md](one-d/first-fit-decreasing.md) | No | No |
| Best-Fit Decreasing | [one-d/best-fit-decreasing.md](one-d/best-fit-decreasing.md) | No | No |
| Multistart Local Search | [one-d/local-search.md](one-d/local-search.md) | No | Yes |
| Column Generation | [one-d/column-generation.md](one-d/column-generation.md) | Yes † | No |
| Auto | [one-d/auto.md](one-d/auto.md) | Sometimes | Indirectly |

† Column generation proves optimality only when it enumerates the full
pattern set under `exact_pattern_limit`. Otherwise it returns the best
solution it found with an LP lower bound.

## 2D algorithms

| Algorithm family | Page | Guillotine-compatible? | Honors `seed` |
| --- | --- | --- | --- |
| MaxRects (5 variants) | [two-d/max-rects.md](two-d/max-rects.md) | No | No |
| Skyline (2 variants) | [two-d/skyline.md](two-d/skyline.md) | No | No |
| Guillotine beam search (7 variants) | [two-d/guillotine.md](two-d/guillotine.md) | Yes | No |
| Shelf heuristics — NFDH / FFDH / BFDH | [two-d/shelf.md](two-d/shelf.md) | Layout yes; flag false | No |
| Multistart MaxRects | [two-d/multi-start.md](two-d/multi-start.md) | No | Yes |
| Rotation Search | [two-d/rotation-search.md](two-d/rotation-search.md) | No | Sampled: Yes |
| Auto | [two-d/auto.md](two-d/auto.md) | Optional | Indirectly |

## 3D algorithms

| Algorithm family | Page | Guillotine-compatible? | Honors `seed` |
| --- | --- | --- | --- |
| Extreme Points (6 variants) | [three-d/extreme-points.md](three-d/extreme-points.md) | No | No |
| Guillotine 3D beam search (7 variants) | [three-d/guillotine.md](three-d/guillotine.md) | Yes | No |
| Layer building (5 variants) | [three-d/layer-building.md](three-d/layer-building.md) | One variant | No |
| Wall and column builders | [three-d/wall-and-column.md](three-d/wall-and-column.md) | No | No |
| Deepest-bottom-left (2 variants) | [three-d/deepest-bottom-left.md](three-d/deepest-bottom-left.md) | No | No |
| Volume-sorted FFD/BFD | [three-d/volume-sorted.md](three-d/volume-sorted.md) | No | No |
| MultiStart, GRASP, LocalSearch | [three-d/meta-strategies.md](three-d/meta-strategies.md) | No | Yes |
| Branch and Bound | [three-d/branch-and-bound.md](three-d/branch-and-bound.md) | No | No |
| Auto | [three-d/auto.md](three-d/auto.md) | Optional | Indirectly |

## Reading notes

**Complexity.** Complexity is given in terms of `n = total number of piece
instances` (i.e., the expanded demand list, with one entry per unit of
`quantity`). Unless otherwise noted, kerf, trim, and multi-stock dimensions do
not change asymptotic complexity — they add a constant factor per placement.

**Determinism.** Every algorithm that does not mention `seed` is
deterministic in the input order. Algorithms that do honor `seed` produce
bit-identical output for the same `(problem, options, seed)` triple. When
`seed = None`, randomized algorithms fall back to a fixed internal default
seed, so runs are still reproducible across processes.

**Ranking.** Solutions are compared lexicographically by a fixed multi-key
tuple per dimension. See each dimension's overview page for the exact
comparator.

**References.** Named variants cite their source in the literature where
the code does so. The primary reference for the 2D MaxRects, Skyline, and
Guillotine families is Jukka Jylänki's survey *A Thousand Ways to Pack the
Bin — A Practical Approach to Two-Dimensional Rectangle Bin Packing* (2010),
which the implementations closely follow.
