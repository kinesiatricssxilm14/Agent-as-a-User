#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
command_name="${1:-quickstart}"
[[ $# -eq 0 ]] || shift

load_env() {
  if [[ ! -f "$ROOT/.env" ]]; then
    "$ROOT/scripts/setup.sh"
  fi
  # shellcheck source=/dev/null
  source "$ROOT/.env"
  export KIT_DIR="$ROOT/study"
  export BENCH_LOCALE=en
  export BENCH_SUITE_ROOT="$ROOT/study/benchmark"
}

require_runtime() {
  command -v tmux >/dev/null || { echo "tmux is required." >&2; exit 1; }
  command -v docker >/dev/null || { echo "Docker is required." >&2; exit 1; }
  docker info >/dev/null 2>&1 || { echo "Start the Docker daemon first." >&2; exit 1; }
}

case "$command_name" in
  verify)
    exec "${PYTHON:-python3}" "$ROOT/scripts/check_artifact.py"
    ;;
  setup)
    exec "$ROOT/scripts/setup.sh"
    ;;
  quickstart)
    "${PYTHON:-python3}" "$ROOT/scripts/check_artifact.py"
    "$ROOT/scripts/setup.sh"
    load_env
    exec "$ROOT/study/run.sh" --operator tester1 --seed 42 --dry-run
    ;;
  build)
    load_env
    require_runtime
    exec "$ROOT/study/build-images.sh" "$@"
    ;;
  human)
    load_env
    require_runtime
    exec "$ROOT/study/run.sh" "$@"
    ;;
  agent)
    load_env
    require_runtime
    exec "$ROOT/study/run-agent.sh" "$@"
    ;;
  *)
    echo "Usage: ./artifact.sh {quickstart|verify|setup|build|human|agent} [options]" >&2
    exit 2
    ;;
esac
