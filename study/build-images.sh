#!/usr/bin/env bash
set -euo pipefail

KIT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
[ -f "$KIT_DIR/.env" ] && source "$KIT_DIR/.env"
# shellcheck source=bench-env.sh
source "$KIT_DIR/bench-env.sh"

BENCH="${BENCH:-$KIT_DIR/packages/evaluation/scripts/bench.sh}"
BENCHMARK="$(resolve_benchmark_root)"
LOG_DIR="$KIT_DIR/build-logs"
mkdir -p "$LOG_DIR"

if [ ! -f "$BENCH" ]; then
  echo "bench.sh not found: $BENCH (run ./install.sh first)" >&2
  exit 1
fi

echo "==> Building images for locale: $(resolve_bench_locale)"
echo "    Root: $BENCHMARK"
echo "    Logs: $LOG_DIR"
echo

FAILED=0
BUILT=0

for project_dir in "$BENCHMARK"/P*/; do
  [ -d "$project_dir" ] || continue
  name="$(basename "$project_dir")"
  log="$LOG_DIR/${name}.log"
  echo "--- $name ---"
  if BENCH_PROJECT="$project_dir" "$BENCH" build --rebuild >"$log" 2>&1; then
    echo "  OK $name"
    BUILT=$((BUILT + 1))
  else
    echo "  FAIL $name (see $log)"
    FAILED=$((FAILED + 1))
  fi
done

echo
echo "Done. ok=$BUILT failed=$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
