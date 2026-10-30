# Column generation (`column_generation`)

`OneDAlgorithm::ColumnGeneration` combines a Gilmore–Gomory restricted master LP,
knapsack pricing, capped pattern enumeration and a demand-state integer search.
It supports exactly one stock type with unlimited inventory; multi-stock or
capped inventory returns `BinPackingError::Unsupported`. Ordinary Auto requests
it only when its source-type and total-quantity escalation gates allow it.
See [actual default controls](../reference.md#onedoptions).

## Pattern model

For usable stock length `U` and kerf `k`, demand `i` has weight
`length_i + k` and the pattern capacity is `U + k`. A pattern of `m` pieces
therefore uses `sum(length_i) + (m-1)*k` material. Trim is removed before packing.
Source demand quantities bound each pattern's copy counts.

The solver retains a local-search incumbent. Initial patterns each fill a stock
with as many copies of one source demand as fit. The column-generation loop solves
the current dual master LP, prices a bounded knapsack using the dual values, and
adds a new improving pattern. It stops on certified pricing convergence, a
repeated pattern, LP failure or the configured round limit.

A restricted master objective alone is **not** a valid lower bound on the full
integer problem: omitted patterns might improve it. The implementation records
an LP bound only after the pricing value is at most `1 + 1e-6`. If that certificate
is unavailable, it uses a conservative length/capacity bound. A duplicate priced
pattern or an exhausted round limit does not independently certify the LP.

## Enumeration and integer search

Pattern enumeration tries to build the full feasible set under
`exact_pattern_limit`. If the cap is hit, that enumeration batch is discarded;
the integer search uses the initial and priced columns, rather than treating a
prefix as a complete pattern set.

The memoized integer search keys on remaining source quantities. It anchors the
next transition on a longest remaining demand, tries patterns containing it,
subtracts their quantities and minimizes consumed stock count. Conservative
length/capacity pruning rejects states that cannot beat the incumbent. This is
exponential search; the pattern cap does not impose a wall-clock deadline or a
separate bound on reachable demand states.

If search cannot reconstruct an improving or tied pattern solution, the solver
returns the heuristic incumbent with a conservative bound and no exact claim.
Otherwise it builds the selected pattern combination. Complete enumeration plus
completed integer search sets `exact = true` and reports the optimum stock count
as the lower bound. Capped enumeration leaves `exact = false`, even if its
restricted column pool happens to find the true optimum.

General input validation, nonempty demand requirements and per-demand fit
checks occur at the public `solve_1d` boundary.

## Result and metrics

- `algorithm` is `column_generation`.
- `lower_bound` is the proven count for a fully enumerated solution, otherwise
  the certified LP value or conservative length/capacity bound.
- `generated_patterns` counts columns added by pricing.
- `enumerated_patterns` records the pattern-set size on the normal reconstructed
  path; heuristic fallback paths may report zero when enumeration was incomplete.
- `explored_states` counts demand states visited by integer search.
- `metrics.notes` explains proof, capped enumeration or heuristic fallback.

A false `exact` flag does not prove suboptimality. Bound equality can establish
stock-count optimality, but does not establish a different material-cost objective.
Use [arc-flow](arc-flow.md) for a mixed-stock bounded pattern search or
[minimum cost](min-cost.md) when material price is the primary objective.

## Controls and cost

`column_generation_rounds` limits LP/pricing rounds. `exact_pattern_limit` caps
the exhaustive pattern batch. Auto's `auto_exact_max_types` and
`auto_exact_max_quantity` govern escalation, not an input limit for an explicitly
selected backend. More patterns and reachable demand states increase time and
memory; full pattern enumeration and integer search can be exponential in the
number of source types. Benchmark the actual workload instead of assuming a
latency ceiling from the Auto thresholds.

## Rust entry point

```rust
use bin_packing::one_d::{OneDAlgorithm, OneDOptions, solve_1d};

// `problem` is an existing single-stock, uncapped OneDProblem.
let solution = solve_1d(problem, OneDOptions {
    algorithm: OneDAlgorithm::ColumnGeneration,
    column_generation_rounds: 64,
    exact_pattern_limit: 50_000,
    ..Default::default()
})?;
if solution.exact {
    println!("proven count: {}", solution.stock_count);
}
```

Source: [exact.rs](../../crates/bin-packing/src/one_d/exact.rs).
