#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PYTHON_BIN="${PYTHON:-python3}"

"$PYTHON_BIN" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)'   || { echo "Python >= 3.10 is required." >&2; exit 1; }
"$PYTHON_BIN" -m venv "$ROOT/.venv"
VENV_PYTHON="$ROOT/.venv/bin/python"

for package in agent-tui evaluation; do
  "$VENV_PYTHON" -m pip install -e "$ROOT/study/packages/$package" --quiet
done
"$VENV_PYTHON" -m pip install -e "$ROOT/study/packages/keyboard-agent[llm]" --quiet

cat >"$ROOT/.env" <<EOF
export ARTIFACT_ROOT="$ROOT"
export BENCH_SUITE_ROOT="$ROOT/study/benchmark"
export BENCH="$ROOT/study/packages/evaluation/scripts/bench.sh"
export HUMAN_OUTPUT="$ROOT/runs"
export CONFIG="$ROOT/study/config.participant.yaml"
export BENCH_LOCALE="en"
export PATH="$ROOT/.venv/bin:$ROOT:\$PATH"
EOF
mkdir -p "$ROOT/runs" "$ROOT/build-logs"
echo "Setup complete. Machine-local configuration: $ROOT/.env"
