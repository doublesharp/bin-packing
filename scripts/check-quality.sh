#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"

require_cmd() {
  if command -v "$1" >/dev/null 2>&1; then
    return 0
  fi

  printf 'error: required command not found: %s\n' "$1" >&2
  case "$1" in
    taplo)
      printf 'install it with: cargo install taplo-cli --locked\n' >&2
      ;;
    typos)
      printf 'install it with: cargo install typos-cli --locked\n' >&2
      ;;
  esac
  exit 127
}

cd "${repo_root}"

# Local opt-in; CI and unconfigured checkouts use the normal workspace.
if [[ -f .scratch.json && ${BIN_PACKING_SCRATCH_ACTIVE:-} != 1 ]]; then
  exec python3 scripts/scratch.py run ./scripts/check-quality.sh "$@"
fi

require_cmd cargo
require_cmd taplo
require_cmd typos
require_cmd python3

PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests
cargo fmt --all --check
taplo fmt --check .
typos
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --manifest-path crates/bin-packing/Cargo.toml --locked --no-deps --document-private-items
