# Auto (`auto`)

`OneDAlgorithm::Auto` — the default 1D algorithm. Runs
[FFD](first-fit-decreasing.md), [BFD](best-fit-decreasing.md), and
[local search](local-search.md) unconditionally, then optionally escalates
to [column generation](column-generation.md) if the instance is small
enough and single-stock. Returns the best candidate under the 1D solution
comparator.

## Mechanism

```
best := solve_ffd(problem, options)
bfd  := solve_bfd(problem, options)
if bfd.is_better_than(best) { best := bfd }

local := solve_local_search(problem, options)
if local.is_better_than(best) { best := local }

if should_attempt_exact(problem, options):
    if exact := solve_exact(problem, options):
        if exact.is_better_than(best) or exact.exact:
            best := exact

return best
```

### Escalation gate

`should_attempt_exact` is conservative: column generation is attempted
only when every condition holds:

- **Single stock type** — `problem.stock.len() == 1`. Column generation
  does not yet handle mixed stock mixes.
- **Unlimited inventory** — `problem.stock[0].available.is_none()`.
  Column generation does not model capped inventory.
- **Small demand surface** —
  `problem.demands.len() <= options.auto_exact_max_types` (default `14`).
- **Small total quantity** —
  `problem.total_quantity() <= options.auto_exact_max_quantity`
  (default `96`).

When any of these fails, Auto returns the best of FFD/BFD/local search.
When the gate opens, Auto calls `solve_exact` but *does not* propagate
its errors — if the exact backend fails for any reason (including the
`BinPackingError::Unsupported` it would raise on a configuration it
cannot handle), Auto silently discards the attempt and keeps the
heuristic incumbent. This makes Auto safe as a drop-in default: it
never makes the call site worse than local search.

### Exact-accepted-even-when-not-strictly-better

The conditional on the exact candidate is
`exact.is_better_than(best) || exact.exact`. The second clause matters:
if the heuristic incumbent and the exact solution agree on bin count,
waste, and cost, they tie under `is_better_than` and the heuristic
wins by first-seen order. But an `exact: true` solution carries a
proof of optimality that the heuristic cannot, and the 1D comparator's
final `!exact` tiebreaker is what makes Auto prefer the proof. See
[1D overview — ranking](README.md#solution-ranking).

## Inventory-aware procurement

When any `Stock1D.available` is set, `solve_1d` performs a
**relaxed-inventory re-solve** on top of the Auto result:

1. The first Auto pass runs the full pipeline honoring inventory caps.
   Its returned layouts reflect what can actually be cut; any cuts that
   ran out of inventory appear in `unplaced`.
2. A second Auto pass runs on a cloned problem with every `available =
   None`, using identical `OneDOptions`. The per-stock
   `used_quantity` from that relaxed solve becomes the
   `required_quantity` in the capped solution's
   `stock_requirements`.
3. `additional_quantity_needed = required_quantity -
   available_quantity` (saturating).

The capped-solve layouts and the relaxed `stock_requirements` are
returned together in a single `OneDSolution`, so downstream callers
can see both the feasible cut list and the procurement gap in one
trip. The relaxed pass is skipped when no stock has an inventory cap —
the `stock_requirements` then mirror actual usage.

Metrics for the relaxed re-solve are *not* returned; the metrics block
always reflects the primary (capped) solve. The relaxed path adds one
diagnostic note:
`"stock requirements estimated from a relaxed-inventory auto solve"`.

## Options

Every option inherited from the underlying algorithms applies:

- From [local search](local-search.md): `multistart_runs`,
  `improvement_rounds`, `seed`.
- From [column generation](column-generation.md):
  `column_generation_rounds`, `exact_pattern_limit`,
  `auto_exact_max_types`, `auto_exact_max_quantity`.

`OneDOptions::default()` is equivalent to
`OneDOptions { algorithm: OneDAlgorithm::Auto, ..Default::default() }`.

## Behavior summary

| Instance shape | What Auto runs | Returns |
| --- | --- | --- |
| Large, multi-stock | FFD + BFD + local | Best of the three heuristics |
| Small, multi-stock | FFD + BFD + local | Exact gate skipped (multi-stock rejected) |
| Small, single-stock, uncapped, ≤ gate thresholds | FFD + BFD + local + exact | The better of (best heuristic, exact result), with a proof if `exact = true` |
| Small, single-stock, **capped** | FFD + BFD + local | Exact gate skipped (capped inventory) |
| Infeasible (some cut > every stock's usable length) | Returns `BinPackingError::Infeasible1D` without running any algorithm | (error) |

## When to use it

- As the **default**. `OneDOptions::default().algorithm` is Auto
  precisely because Auto is safe across all instance shapes and
  escalates to the exact backend automatically when the instance is
  small enough to benefit.
- When you want **the best answer the library can produce** without
  having to manually pick an algorithm.
- When you want **optimality proofs on small instances** without
  writing gate logic yourself — Auto applies the exact gate for you.

## When to bypass it

- When you know **you don't need the exact backend** and want to skip
  the LP cost on medium instances where Auto would try and reject it.
  Call `LocalSearch` directly.
- When you need **predictable worst-case timing** — Auto can spend
  most of its budget in the exact backend if the gate opens. Call
  the specific heuristic you want if you need a latency ceiling.

## Rust entry point

```rust
use bin_packing::one_d::{OneDOptions, OneDProblem, solve_1d};

// Auto is the default algorithm.
let solution = solve_1d(problem, OneDOptions::default())?;

// If you want to tune the escalation thresholds:
let solution = solve_1d(
    problem,
    OneDOptions {
        auto_exact_max_types: 20,
        auto_exact_max_quantity: 200,
        seed: Some(42),
        ..Default::default()
    },
)?;
```

Source: [`crates/bin-packing/src/one_d/mod.rs`](../../crates/bin-packing/src/one_d/mod.rs)
(see `solve_1d`, `solve_auto`, `estimate_required_stock_counts`).
