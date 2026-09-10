#!/usr/bin/env bash
# Exercise dimensions independently so workspace feature unification cannot hide gaps.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
# Local opt-in; CI and unconfigured checkouts use the normal workspace.
if [[ -f .scratch.json && ${BIN_PACKING_SCRATCH_ACTIVE:-} != 1 ]]; then
  exec python3 scripts/scratch.py run ./scripts/check-features.sh "$@"
fi

for features in '' one-d two-d three-d one-d,two-d one-d,three-d parallel one-d,two-d,three-d; do
  args=(--no-default-features --locked)
  if [[ -n "$features" ]]; then args+=(--features "$features"); fi
  printf 'Checking Rust features: %s\n' "${features:-none}"
  cargo test -p bin-packing --all-targets "${args[@]}"
  cargo clippy -p bin-packing-node --all-targets "${args[@]}" -- -D warnings
done
for features in '' json one-d two-d three-d one-d,json two-d,json three-d,json one-d,two-d,three-d,json; do
  args=(--no-default-features --locked)
  if [[ -n "$features" ]]; then args+=(--features "$features"); fi
  printf 'Checking WASM features: %s\n' "${features:-none}"
  cargo clippy -p bin-packing-wasm --all-targets "${args[@]}" -- -D warnings
done
cargo fmt --manifest-path fuzz/Cargo.toml -- --check
cargo clippy --manifest-path fuzz/Cargo.toml --locked --bins -- -D warnings
