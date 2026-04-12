# One-dimensional cutting stock

The 1D solver packs a list of integer-length cuts onto a list of integer-length
stock pieces, minimizing the number of stock pieces consumed. It handles
*kerf* (material lost to the saw between adjacent cuts), *trim* (unusable
material removed from the stock length before packing), multiple stock
types with independent cost and optional inventory caps, and fractional
`cost` for mixed-priced stock mixes. The entry point is `solve_1d` and the
algorithm is chosen via `OneDOptions.algorithm`.

## Algorithms

1. **[First-Fit Decreasing (`first_fit_decreasing`)](first-fit-decreasing.md)** —
   sort cuts descending, place each into the first open bin that fits.
   Deterministic, O(n²).
2. **[Best-Fit Decreasing (`best_fit_decreasing`)](best-fit-decreasing.md)** —
   same sort, but place each cut into the open bin with the tightest fit
   after placement. Typically better than FFD on mixed-size demand.
3. **[Multistart Local Search (`local_search`)](local-search.md)** —
   seeded from both FFD and BFD, then repeatedly perturbs the piece order
   and rebuilds, followed by a bin-elimination repair pass.
4. **[Column Generation (`column_generation`)](column-generation.md)** —
   exact cutting-stock backend using dual-LP pricing plus pattern dynamic
   programming. Proves optimality on single-stock, unlimited-inventory
   instances under the pattern-enumeration cap.
5. **[Auto (`auto`)](auto.md)** — the default. Runs FFD, BFD, and local
   search, then optionally escalates to column generation when the instance
   is small enough. Returns the best candidate under the 1D comparator.

## Problem shape

A 1D problem is a list of `Stock1D` entries and a list of `CutDemand1D`
entries. Every dimension is a positive `u32`, validated at the public
boundary.

```rust
pub struct Stock1D {
    pub name: String,
    pub length: u32,            // raw length before trim
    pub kerf: u32,              // material lost per saw cut (default 0)
    pub trim: u32,              // material removed before packing (default 0)
    pub cost: f64,              // per-unit cost (default 1.0)
    pub available: Option<usize>, // optional inventory cap
}

pub struct CutDemand1D {
    pub name: String,
    pub length: u32,
    pub quantity: usize,
}
```

## Kerf and trim accounting

Every 1D algorithm uses the same two helpers on `Stock1D`:

- `usable_length() = length.saturating_sub(trim)` — the capacity a packed
  bin may fill.
- `adjusted_capacity() = usable_length() + kerf` — used by the exact
  backend for pattern weights. Pattern weight for a cut is
  `cut.length + stock.kerf`; the `+ kerf` in the capacity cancels the
  double-counted kerf after the last cut, so a pattern with $k$ cuts
  accounts for exactly $k-1$ kerfs worth of saw loss.

Validation rejects `length = 0`, `length > MAX_DIMENSION` (`1 << 30`),
or `trim >= length` (via `usable_length == 0`). Non-finite or negative
`cost` is rejected.

## Multi-stock selection

When multiple `Stock1D` entries are declared, every algorithm uses the same
**`choose_new_stock` comparator** whenever it has to open a fresh bin:

1. Only stocks where `usable_length() >= piece.length` and inventory is not
   yet exhausted (`available.map(|a| used < a)`) are eligible.
2. Among eligible stocks, pick the one minimizing the lexicographic tuple
   `(waste, cost, length)`:
   - **`waste = usable_length - piece.length`** — tightest fit first.
   - **`cost`** — break ties toward cheaper stock.
   - **`length`** — break remaining ties toward the smaller raw stock
     (prefer using up short offcuts before long ones).

FFD, BFD, and local search all use this comparator. Column generation does
not support multi-stock; it returns `BinPackingError::Unsupported` if given
more than one stock entry. See [column-generation.md](column-generation.md).

## Inventory caps and procurement estimates

When any `Stock1D.available` is `Some(_)`, `solve_1d` performs an extra
**relaxed-inventory re-solve**:

1. Run the requested algorithm against the original problem, honoring
   caps. The returned layouts and any `unplaced` cuts reflect the actual
   constrained solution.
2. Internally clone the problem with every `available = None`, run `Auto`
   on it, and record per-stock usage.
3. Fill in the returned `OneDSolution.stock_requirements` with capped
   usage as `used_quantity` and relaxed usage as `required_quantity`, so
   callers can see how many additional units of each stock type they
   would need to buy to satisfy the whole cut list.

The relaxed pass is skipped when no stock has an `available` cap — the
`stock_requirements` then just mirror the `used_quantity` from the primary
solve.

## Solution ranking

Solutions are compared lexicographically on the tuple:

```
(unplaced.len(), stock_count, total_waste, total_cost, !exact)
```

- **`unplaced.len()`** — a solution that places more cuts always wins.
- **`stock_count`** — fewer stock pieces wins. This is the primary
  optimization target.
- **`total_waste`** — aggregate wasted length across all bins.
- **`total_cost`** — sum of `Stock1D.cost` for each consumed bin.
  Compared with `f64::total_cmp` so `NaN` is ordered deterministically.
- **`!exact`** — if everything else ties, a proven-optimal solution
  (`exact = true`) beats a heuristic candidate. This is what lets
  [Auto](auto.md) prefer a column-generation result over an identical-
  quality heuristic.

The `is_better_than` comparator is defined once on `OneDSolution` and
reused by every algorithm that picks among its own candidates.

## Reproducibility

Local search is the only 1D algorithm that consults `OneDOptions.seed`.
When `seed = None`, it falls back to the fixed constant
`0x4249_4E50_4143_4B30` (ASCII `"BINPACK0"`), so runs are still
reproducible across processes. FFD, BFD, and column generation are fully
deterministic in the input order regardless of `seed`.
