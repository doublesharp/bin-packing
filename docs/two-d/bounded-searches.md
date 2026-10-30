# Bounded Rust packing searches and forecast reuse

These opt-in Rust functions preserve ordinary entry points and are not variants
of `TwoDAlgorithm` or JavaScript exports. Bounds, candidate restrictions and
incumbent retention are part of their contracts. Metrics describe work done;
none of these searches proves unrestricted packing optimality.

## Whole-sheet pattern recombination

`two_d::solve_2d_recombined(problem, options)` collects complete physical sheet
layouts from the Auto portfolio, then chooses whole patterns with exact source
quantities and hard stock caps. It retains the best portfolio incumbent. Repeated
demand labels are indexed internally; `options.algorithm` is replaced by the
portfolio's algorithms.

Recombination supports 16 demand types, 16 stocks, 96 parts and 256 patterns.
Its state budget is `min(max(beam_width, 1) * 512, 25_000)`. Above those pool/input
limits it falls back to ordinary Auto, up to a hard public limit of 128 parts.
Beam width and multistart runs must be at most 32, including fallback. A restricted
pool cannot establish global optimality. Memoization is disabled when possible
cost sums are not exactly representable integers.

## Remaining-demand placement and mixed rotations

`two_d::solve_2d_demand_aware(problem, options)` scores at most 16 MaxRects
geometric candidates per placement using remaining-demand opportunity loss.
Overlapping free rectangles are not summed as independent capacity.

`two_d::solve_2d_mixed_rotations(problem, options)` tries bounded vectors of
rotated-copy counts within repeated demand rows, including seeded item orders.
It attempts at most 128 packing/order trials. Exhausting those count vectors
still does not enumerate every layout.

Both retain the requested ordinary baseline and return it unchanged for
guillotine-required jobs. Bounds are 128 parts, 64 demands, eight stocks and
eight rotatable types; beam width is 1–32 and multistart runs 1–16. Unspecified
seeds become zero. Stock names must be unique. For these searches and pattern
recombination, largest stock area times total quantity must fit in `u64`.

## Forecast-aware recoverable offcuts

`two_d::reuse::evaluate_reuse(job, forecast, options)` values terminal offcuts
from a physical saw decomposition. `job` is a `SawJob2D` with explicit stock and
demand provenance. Maximal free rectangles from ordinary layout metrics are not
necessarily physically recoverable, disjoint pieces.

The evaluator greedily places weighted future parts with required kerf and
vertical-then-horizontal splits, retaining disjoint residual rectangles. It
supports 32 layouts, 128 current placements, 64 forecast shapes and up to 512
future placement attempts. Forecast rows may contain up to 4,096 pieces in total.
Weights order future demand; the supplied forecast is not a demand prediction or
a proof of future value. Mixed blade kerfs need separate jobs.

`two_d::reuse::solve_2d_with_reuse(problem, options, forecast, reuse)` evaluates
a bounded candidate family while retaining the ordinary baseline. Improved
forecast score must not worsen unplaced quantity. Sheet count and material cost
default to the baseline ceilings; explicit overrides can relax either, with
inventory caps always hard. Equal forecast scores use the ordinary comparator.
The baseline must admit a physical saw decomposition.

Current jobs support eight stocks, 64 demands and 128 parts, beam width 1–32 and
multistart runs 1–16. The rotation threshold is capped at eight; default seed is
zero. `max_candidate_solves` is 1–8 and `max_future_parts` is 1–512. The
largest stock area times quantity must fit in `u64`. Material ceilings and
unsupported physical decompositions fail with `ReuseError2D`.

See [the recorded experiment findings](../experiments/2026-10-14-packing-findings.md)
for qualified improvements and limits. Sources: [bounded_search.rs](../../crates/bin-packing/src/two_d/bounded_search.rs),
[pattern_pool.rs](../../crates/bin-packing/src/two_d/pattern_pool.rs),
[rotation_search.rs](../../crates/bin-packing/src/two_d/rotation_search.rs) and
[reuse.rs](../../crates/bin-packing/src/two_d/reuse.rs).

Effective option defaults are recorded in the [generated reference](../reference.md).
