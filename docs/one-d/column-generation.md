# Column Generation (`column_generation`)

`OneDAlgorithm::ColumnGeneration` — the exact cutting-stock backend. Solves
the Gilmore-Gomory formulation with dual-simplex pricing, refines the
column set with exhaustive pattern enumeration under a cap, then runs a
demand-state dynamic program to pick the best integer combination of
patterns. Proves optimality on single-stock, unlimited-inventory instances
when the pattern-enumeration cap is not exceeded.

## When it runs

This backend is directly accessible via
`OneDAlgorithm::ColumnGeneration`, and is also the third tier of the
default [Auto](auto.md) mode when the instance is small enough
(`demands.len() <= auto_exact_max_types` and
`total_quantity() <= auto_exact_max_quantity`).

## Preconditions and unsupported configurations

The backend is **only defined for single-stock, unlimited-inventory
problems**. Specifically, it returns `BinPackingError::Unsupported` when:

- `problem.stock.len() != 1` — multi-stock is not yet supported.
- `problem.stock[0].available.is_some()` — inventory caps are not
  supported; pattern search assumes an unlimited pool.

These restrictions are enforced at the top of `solve_exact`, before any
LP work happens. When called from Auto, the preconditions are what
gate the escalation — Auto silently skips column generation if the
instance is multi-stock or capped.

## Mechanism

### Step 1 — extract weights and capacity

Given the single stock `S` with usable length `U` and kerf `k`:

- **Weights.** Each demand `i` contributes a pattern weight of
  `w_i = length_i + k`. The `+k` includes the kerf that would follow
  the cut in a packed pattern.
- **Capacity.** The pattern capacity is
  `C = S.usable_length + k = U + k`. The extra `+k` cancels the
  double-counted kerf on the *final* cut of a pattern, so a pattern
  holding `m` cuts correctly uses `sum(length_i) + (m-1) · k` of the
  stock.
- **Quantities.** Each `demand_i` contributes `quantity_i` to a demand
  vector used by both the LP and the DP.

### Step 2 — seed with initial patterns

`initial_patterns` builds one "maximal single-item" pattern per demand:
the pattern `{i: floor(C / w_i).min(quantity_i)}` packing as many copies
of demand `i` as fit in one stock piece. Demands whose single-item
pattern is empty (too long to fit at all — which
`ensure_feasible_demands` already rejects at the outer API boundary)
are skipped.

### Step 3 — column-generation loop

For `round in 0..column_generation_rounds` (default `32`):

1. **Solve the restricted master LP**, formulated as the dual. The
   primary is "minimize sum of pattern variables subject to demand
   coverage"; the dual is solved by an in-place simplex (`solve_dual_lp`)
   using a tableau with a 10,000-iteration pivot cap. On success the
   dual returns an objective value and a per-demand dual price vector.
2. **Record the LP lower bound.** `lp_lower_bound = dual.objective` is
   the LP relaxation of the Gilmore-Gomory master — a valid lower bound
   on the integer optimum for the current restricted pattern set.
3. **Price a new column.** `best_pricing_pattern` solves a bounded-item
   knapsack over capacity `C` using dual prices `y_i` as profits and
   weights `w_i`, restricted by `quantities`. The knapsack itself is
   a memoized recursive DP in `(index, remaining_capacity)`.
4. **Check reduced cost.** If the knapsack value is ≤ `1 + 1e-6` (the
   columns's reduced cost is not strictly positive), stop. The LP is
   optimal over the current column pool.
5. Otherwise, if the priced pattern is **new** (not already in `seen`),
   append it to `patterns` and increment `generated_patterns`. If it's
   already in the set, stop — the pricing subproblem has returned to an
   existing column, indicating LP convergence.

The loop also terminates early if `solve_dual_lp` fails (exceeds the
pivot cap or encounters an unbounded dual), in which case the exact
backend falls back to returning the heuristic incumbent with the best
LP bound it obtained so far.

### Step 4 — exhaustive pattern enumeration (capped)

`enumerate_patterns` generates **every** feasible pattern up to
`exact_pattern_limit` (default `25_000`). It is a recursive generator
over `(index, remaining_capacity, current_counts)` that short-circuits
when the pattern count exceeds the cap.

- If enumeration completes under the cap, the resulting pattern set is
  **complete** — every integer solution to the cutting-stock problem
  can be expressed as a combination of these patterns. The
  `enumerated_all` flag is set.
- If enumeration hits the cap, `enumerated_all` stays false and the
  DP will only search the column-generation patterns plus whatever
  fragment of the enumeration it saw.

Every enumerated pattern that is not already in `seen` is appended
to `patterns`.

### Step 5 — pattern dynamic program

`exact_search` runs a memoized DP over demand states. State: the
remaining demand vector. Transition: subtract any pattern that "fits"
the remaining demand. Objective: minimize the total number of patterns
(== total number of stock pieces) used.

Lower-bound pruning cuts the search whenever the LP relaxation
restricted to the current remaining demand exceeds the incumbent best
count. The memoized cache keys on the remaining-demand vector.

On termination the DP returns the optimal pattern multiset (under the
columns available to it) and the reconstruction walks the cache
backward to recover which patterns were picked.

### Step 6 — build the solution

The final `OneDSolution` fills bins by instantiating one `PackedBin`
per selected pattern, in pattern order, and reports:

- `algorithm = "column_generation"`
- `exact = enumerated_all` — `true` iff pattern enumeration completed
  under the cap and the DP ran over the complete pattern set.
- `lower_bound = Some(lp_lower_bound)` when the LP produced a positive
  objective, otherwise `Some(best_count as f64)` as a degenerate
  fallback.
- `metrics.iterations = generated_patterns + 1`
- `metrics.generated_patterns` — columns added by the pricing loop.
- `metrics.enumerated_patterns` — total pattern-set size used by the DP
  (`0` when the enumeration cap was hit, since the set is then
  incomplete).
- `metrics.notes` — includes `"column generation supplies the LP lower
  bound"` and either `"pattern dynamic programming proved optimality"`
  or `"pattern enumeration hit the configured cap; result is
  best-known"` depending on `enumerated_all`.

## Options that matter

- **`column_generation_rounds`** (default `32`) — maximum
  column-generation iterations. Larger values let the LP converge on
  pathological instances; 32 is sufficient for almost every realistic
  cutting-stock problem.
- **`exact_pattern_limit`** (default `25,000`) — cap on exhaustive
  pattern enumeration. Raising this lets the DP see a larger (possibly
  complete) pattern set on big instances, at the cost of quadratic-ish
  memory in the cap.
- **`auto_exact_max_types`** (default `14`) — when routed via Auto,
  column generation is only attempted if `demands.len() <= 14`.
- **`auto_exact_max_quantity`** (default `96`) — and only if
  `total_quantity() <= 96`. Both gates exist because column generation's
  cost grows quickly in the number of distinct sizes and total cuts.

## Complexity

- **LP phase**: `O(column_generation_rounds × (variables × constraints))`
  per pivot, times the number of pivots the dual simplex needs to
  converge. On the sizes Auto targets this is essentially instant.
- **Pattern enumeration**: exponential in the number of distinct
  demand types. The `exact_pattern_limit` cap is a hard ceiling.
- **DP**: bounded by the memoization cache, which has one entry per
  reachable demand state. The number of reachable states is exponential
  in demand types in the worst case but tightly pruned by the LP lower
  bound in practice.
- **Space**: dominated by the pattern set and the DP cache.

## Optimality guarantee

`solution.exact == true` **implies proven optimality** for the
single-stock, unlimited-inventory instance: the DP searched the full
feasible pattern set, the incumbent was compared against the LP lower
bound, and no combination of patterns produces a smaller bin count.

`solution.exact == false` does not imply suboptimality — the DP may
have found the optimum even when enumeration was capped — but the
library cannot prove it, so the flag stays `false`. The LP lower bound
is always valid regardless of `exact`, and callers can compare
`lower_bound` against `stock_count` to know how far the result can be
from optimum (if they differ by less than 1.0, the heuristic is
already optimal under rounding).

## When to use it

- **Any instance with ≤ 14 distinct demand types and ≤ 96 total
  cuts**. This is the regime Auto escalates into by default and where
  the exact backend typically completes in under a second.
- When you want **a provable lower bound** alongside your solution,
  even if the DP itself is capped. The LP bound is always reported.
- When you need to **prove that a heuristic is optimal** — run the
  exact backend, compare `stock_count` against `lower_bound.ceil()`,
  and if they match you have a proof.

## When to avoid it

- **Multi-stock problems** — unsupported; the backend returns
  `BinPackingError::Unsupported`.
- **Inventory-capped problems** — same reason.
- **Very large instances** (dozens of distinct sizes, thousands of
  cuts) where exhaustive enumeration is infeasible. Fall back to
  [local search](local-search.md) and accept a heuristic answer.

## Rust entry point

```rust
use bin_packing::one_d::{OneDAlgorithm, OneDOptions, OneDProblem, solve_1d};

let solution = solve_1d(
    problem,
    OneDOptions {
        algorithm: OneDAlgorithm::ColumnGeneration,
        column_generation_rounds: 64,
        exact_pattern_limit: 50_000,
        ..Default::default()
    },
)?;

if solution.exact {
    println!("proven optimal: {} bins", solution.stock_count);
} else if let Some(bound) = solution.lower_bound {
    println!("{} bins; lower bound {}", solution.stock_count, bound);
}
```

Source: [`crates/bin-packing/src/one_d/exact.rs`](../../crates/bin-packing/src/one_d/exact.rs)
(see `solve_exact`, `initial_patterns`, `solve_dual_lp`,
`best_pricing_pattern`, `enumerate_patterns`, `exact_search`,
`build_solution_from_patterns`).
