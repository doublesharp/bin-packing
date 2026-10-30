# Compressed pattern DAG search

`one_d::solve_1d_arc_flow(problem, options)` is an opt-in Rust entry point.
It preserves the ordinary count-first ranking while searching mixed-stock
patterns with kerf, trim, finite inventory and source-row identity. This is
exponential integer search over a compressed graph, not a polynomial flow solver.

The ordinary Auto result supplies an incumbent and relaxed-inventory procurement
estimate. Per-stock DAG paths stream patterns during integer search; a pool of
all patterns is not materialized. Candidate solutions must improve the complete
ordinary ranking before replacing the incumbent. `options.algorithm` is ignored.

`exact_pattern_limit` bounds combined graph nodes/arcs and memo records.
`exact_pattern_limit * max(column_generation_rounds, 1)` bounds integer-search
states and arc visits separately from initial incumbent/procurement construction.
Input supports at most 128 demand types, 128 stocks and 4,096 total cuts; search
selects at most 128 bars. Graph/search/bar truncation retains the best incumbent
and leaves `exact` false. `exact` is true only when the full supported integer
search completes. There is no unrestricted proof from a restricted pattern pool.

Metrics record graph size, represented path counts, streamed pattern visits,
storage estimates, work and truncation. `enumerated_patterns` may visit the same
pattern in several demand states. Fractional costs disable memoization that could
otherwise alter floating-point comparison. Procurement remains an estimate from
ordinary Auto, not a cost-optimal purchasing recommendation.

Use [minimum cost](min-cost.md) to prioritize material price, or
[explicit objectives](../objectives.md) for profit, mandatory quantities and
open length. Default controls are in the [generated reference](../reference.md).

Source: [arc_flow.rs](../../crates/bin-packing/src/one_d/arc_flow.rs).
