# AGENTS.md

Rules for contributing to this Rust bin-packing workspace. Keep the crate publishable, correct, and idiomatic.

## Safety & correctness

- `#![forbid(unsafe_code)]` stays on. No `unsafe` blocks.
- No `.unwrap()`, `todo!()`, or `dbg!()` anywhere (workspace denies these via `clippy::unwrap_used`, `clippy::todo`, `clippy::dbg_macro`). In library code, no `.expect()` or `panic!()` either — return `Result` and use `debug_assert!` for invariants. Tests may use `.expect("…")` for clarity; prefer `assert!(matches!(...))` when inspecting error cases.
- Never suppress a lint with `#[allow(...)]` without a recorded reason. If a lint is firing, fix the root cause. Don't add empty allow attributes "just in case" — if the lint isn't firing, the allow is noise.
- Never silently discard a fallible result. `let _ = expr;` is banned for `Result`/`bool` returns — either handle, assert, or propagate.
- Never mutate a collection while iterating over indices into it. Use `retain`, `drain`, reverse-order removal, or build a fresh `Vec`.
- Integer arithmetic on user-supplied dimensions must use `saturating_*` or widen to `u64` before multiplying. `u32 * u32` on width/height is forbidden — widen first.
- Validate inputs at public boundaries (`TwoDProblem::validate`, `OneDProblem::validate`). Fail fast with structured `BinPackingError` variants.

## API & error design

- `BinPackingError` is `#[non_exhaustive]`. Any new variant is still a minor bump, but callers must use `_ =>` arms.
- Prefer structured error variants over `InvalidInput(String)` when the caller might want to match on the failure.
- Public types must derive `Debug` and `Clone` where cheap. `Serialize`/`Deserialize` where useful for wire formats.
- Public items need doc comments. `#![warn(missing_docs)]` is on — treat warnings as errors.
- Re-exports are organized by module (`one_d`, `two_d`) rather than dumped at the crate root. The crate root re-exports the error type only.
- Naming is consistent across 1D/2D (e.g., `CutDemand1D` / `RectDemand2D` — keep the pattern).

## Performance

- Avoid `Vec::remove(i)` inside loops; it's O(n) per call. Use `retain`, `swap_remove`, `drain`, or collect-then-apply.
- Don't clone owned data (especially `String`) in hot loops. Borrow, index into a shared slice, or use `Rc` if ownership must be shared.
- Document algorithmic complexity on non-obvious helpers (`prune_contained_rects`, skyline merges, etc.).

## Code organization

- Cross-algorithm helpers (e.g., `orientations()` for 2D items) live in `two_d/model.rs` as methods on the instance type, not copy-pasted per algorithm file.
- Keep private helpers `pub(crate)` or tighter. Only the intended API surface is `pub`.

## Tooling

- `rustfmt` with the repo's `rustfmt.toml` (edition 2024, Unix newlines, `use_small_heuristics = "Max"`).
- Workspace clippy lints are `deny`: `unwrap_used`, `dbg_macro`, `todo`. Treat `cargo clippy --workspace --all-targets -- -D warnings` as the gate.
- Before claiming work is complete: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- MSRV is declared in `[workspace.package]`. Don't use features newer than that without bumping it.

## Git

- One logical change per commit. Correctness fixes, API changes, and publishing metadata should be separate commits.
- Never run destructive git commands (`reset --hard`, `push --force`, branch deletions) without explicit instructions.
