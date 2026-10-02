#!/usr/bin/env bash
# Optional: install bench-oracle CLI to PATH. bench.sh works without this (uses python3 -m oracle.cli).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EVAL_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

bench_pick_python() {
  local candidates=(python3 python3.12 python3.11 python3.10 python3.9)
  local py min_ok=0
  for py in "${candidates[@]}"; do
    if ! command -v "$py" >/dev/null 2>&1; then
      continue
    fi
    if "$py" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 9) else 1)'; then
      echo "$py"
      return 0
    fi
    min_ok=1
  done
  if [[ "$min_ok" -eq 1 ]]; then
    echo "Need Python 3.9+. Found:" >&2
    for py in "${candidates[@]}"; do
      command -v "$py" >/dev/null 2>&1 && "$py" --version >&2 || true
    done
  else
    echo "python3 not found in PATH" >&2
  fi
  return 1
}

PYTHON="$(bench_pick_python)" || exit 1

echo "Installing bench-oracle from ${EVAL_ROOT} …"
echo "Using: $("$PYTHON" --version)"

"$PYTHON" -m pip install -e "${EVAL_ROOT}"

echo
echo "Done. bench-oracle is on PATH."
echo
echo "Note: bench.sh does NOT require this step — it runs via python3 -m oracle.cli."
echo "  export BENCH_PROJECT=/path/to/Finshed/P01-aptui"
echo "  ${SCRIPT_DIR}/bench.sh start T01 --human"
