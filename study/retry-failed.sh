#!/usr/bin/env bash
set -euo pipefail

KIT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
[ -f "$KIT_DIR/.env" ] && source "$KIT_DIR/.env"
# shellcheck source=bench-env.sh
source "$KIT_DIR/bench-env.sh"

BENCHMARK="$(resolve_benchmark_root)"
BENCH="${BENCH:-$KIT_DIR/packages/evaluation/scripts/bench.sh}"
OUTPUT="${HUMAN_OUTPUT:-$KIT_DIR/runs}"
CONFIG="${CONFIG:-$KIT_DIR/config.participant.yaml}"

ARGS=(
  --root "$BENCHMARK"
  --bench "$BENCH"
  --output "$OUTPUT"
  --retry-failed
)
if [ -f "$CONFIG" ]; then
  ARGS+=(--config "$CONFIG")
fi
if [ -n "${OPERATOR:-}" ]; then
  ARGS+=(--operator "$OPERATOR")
fi

exec keyboard-agent human run-suite "${ARGS[@]}" "$@"
