#!/usr/bin/env bash
# Full consumer verification. TSC_PATH and PLAYWRIGHT_PATH may point to existing tools.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
if [[ $# -gt 1 || (${1:-} != '' && ${1:-} != '--skip-build') ]]; then
  printf 'Usage: scripts/check-bindings.sh [--skip-build]\n' >&2
  exit 2
fi
if [[ ${1:-} != '--skip-build' ]]; then
  pnpm --dir bindings/node install --frozen-lockfile
  npm --prefix bindings/node run build
  npm --prefix bindings/wasm run build
fi
npm --prefix bindings/node test
npm --prefix bindings/wasm test
node bindings/test/package-native.cjs
node bindings/test/package-wasm.mjs
node bindings/test/package-types.mjs ${TSC_PATH:+"$TSC_PATH"}
node bindings/test/solver-finder.cjs --cases 300
node bindings/test/package-browser.cjs ${PLAYWRIGHT_PATH:+"$PLAYWRIGHT_PATH"}
