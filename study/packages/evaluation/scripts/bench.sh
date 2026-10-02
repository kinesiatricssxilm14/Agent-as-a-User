#!/usr/bin/env bash
# Unified TUI-Bench workflow. State: .oracle/current_session.json + session_Txx.json + Txx_fingerprints.json
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=./_bench_common.sh
source "${SCRIPT_DIR}/_bench_common.sh"

BENCH_PROJECT_CACHED=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -C|--project)
      [[ $# -ge 2 ]] || { bench_usage; exit 2; }
      BENCH_PROJECT_CACHED="$(bench_resolve_project "$2")"
      shift 2
      ;;
    -h|--help)
      bench_usage
      exit 0
      ;;
    -*)
      break
      ;;
    *)
      break
      ;;
  esac
done

CMD="${1:-}"
[[ -n "$CMD" ]] || { bench_usage; exit 2; }
shift

case "$CMD" in
  setup)
    exec "${SCRIPT_DIR}/setup.sh"
    ;;

  list)
    PROJECT="$(bench_require_project)" || exit 2
    bench_oracle_project "$PROJECT" list
    ;;

  build)
    PROJECT="$(bench_require_project)" || exit 2
    ARGS=(build)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --rebuild|--human) ARGS+=("$1") ;;
        -h|--help) bench_usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; bench_usage; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  start)
    PROJECT="$(bench_require_project)" || exit 2
    [[ $# -ge 1 ]] || { echo "Usage: bench.sh start T01 [--human]" >&2; exit 2; }
    TASK_ID="$1"
    shift
    ARGS=(session start "$TASK_ID" --human)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --human) ;;
        --json) ARGS+=(--json) ;;
        --rebuild) ARGS+=(--rebuild) ;;
        --no-build) ARGS+=(--no-build) ;;
        --skip-docker) ARGS+=(--skip-docker) ;;
        --docker-image)
          [[ $# -ge 2 ]] || exit 2
          ARGS+=(--docker-image "$2")
          shift
          ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  start-all)
    PROJECT="$(bench_require_project)" || exit 2
    ARGS=(session start-all --human)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --human) ;;
        --json) ARGS+=(--json) ;;
        --rebuild) ARGS+=(--rebuild) ;;
        --no-build) ARGS+=(--no-build) ;;
        --skip-docker) ARGS+=(--skip-docker) ;;
        --docker-image)
          [[ $# -ge 2 ]] || exit 2
          ARGS+=(--docker-image "$2")
          shift
          ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  next)
    PROJECT="$(bench_require_project)" || exit 2
    ARGS=(session next --human)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --human) ;;
        --json) ARGS+=(--json) ;;
        --rebuild) ARGS+=(--rebuild) ;;
        --no-build) ARGS+=(--no-build) ;;
        --skip-docker) ARGS+=(--skip-docker) ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  current)
    PROJECT="$(bench_require_project)" || exit 2
    bench_oracle_project "$PROJECT" session current --human
    ;;

  status)
    PROJECT="$(bench_require_project)" || exit 2
    ARGS=(session status)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --run-id)
          [[ $# -ge 2 ]] || exit 2
          ARGS+=(--run-id "$2")
          shift
          ;;
        --json) ARGS+=(--json) ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  task)
    PROJECT="$(bench_require_project)" || exit 2
    TASK_ID="$(bench_current_field "$PROJECT" active_task_id)"
    SESSION_FILE="$(bench_task_session_file "$PROJECT" "$TASK_ID")"
    python3 -c "
import json, sys
d = json.load(open('$SESSION_FILE'))
print(json.dumps({
    'task_id': d.get('task_id'),
    'observation': d.get('observation'),
    'description': d.get('description'),
    'fingerprints': d.get('fingerprints', {}),
}, indent=2, ensure_ascii=False))
"
    ;;

  use)
    PROJECT="$(bench_require_project)" || exit 2
    SESSION_ID="$(bench_active_session_id "$PROJECT")"
    exec agent-tui use "$SESSION_ID"
    ;;

  verify)
    PROJECT="$(bench_require_project)" || exit 2
    ARGS=(session verify --human --quiet)
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --human|--quiet) ;;
        --json) ARGS+=(--json) ;;
        --run-id)
          [[ $# -ge 2 ]] || exit 2
          ARGS+=(--run-id "$2")
          shift
          ;;
        --agent-answer)
          [[ $# -ge 2 ]] || exit 2
          ARGS+=(--agent-answer "$2")
          shift
          ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
      esac
      shift
    done
    bench_oracle_project "$PROJECT" "${ARGS[@]}"
    ;;

  stop)
    PROJECT="$(bench_require_project)" || exit 2
    bench_oracle_project "$PROJECT" session stop --human
    ;;

  *)
    echo "Unknown command: $CMD" >&2
    bench_usage
    exit 2
    ;;
esac
