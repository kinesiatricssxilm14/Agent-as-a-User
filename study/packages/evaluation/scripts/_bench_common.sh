# Shared helpers for evaluation/scripts/bench.sh
# shellcheck shell=bash

_bench_common_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_EVAL_ROOT="$(cd "${_bench_common_dir}/.." && pwd)"
BENCH_SCRIPTS_DIR="${_bench_common_dir}"

bench_setup_env() {
  export PYTHONPATH="${BENCH_EVAL_ROOT}${PYTHONPATH:+:$PYTHONPATH}"
  export BENCH_ORACLE_LIB="${BENCH_EVAL_ROOT}/scripts/oracle_lib.sh"
}

bench_oracle() {
  bench_setup_env
  python3 -m oracle.cli "$@"
}

bench_oracle_project() {
  local project_dir="$1"
  shift
  bench_oracle --project-dir "$project_dir" "$@"
}

bench_resolve_project() {
  if [[ $# -lt 1 || -z "${1:-}" ]]; then
    echo "Missing project directory" >&2
    return 2
  fi
  cd "$1" && pwd
}

bench_require_project() {
  if [[ -n "${BENCH_PROJECT_CACHED:-}" ]]; then
    echo "$BENCH_PROJECT_CACHED"
    return 0
  fi
  if [[ -n "${BENCH_PROJECT:-}" ]]; then
    BENCH_PROJECT_CACHED="$(bench_resolve_project "$BENCH_PROJECT")"
    echo "$BENCH_PROJECT_CACHED"
    return 0
  fi
  cat >&2 <<EOF
Missing project directory.

  export BENCH_PROJECT=/path/to/Finshed/P01-aptui
  bench.sh start-all

Or:

  bench.sh -C /path/to/Finshed/P01-aptui start-all
EOF
  return 2
}

bench_current_json() {
  local project_dir="$1"
  python3 -c "
import json, sys
from pathlib import Path
p = Path('$project_dir') / '.oracle' / 'current_session.json'
if not p.is_file():
    sys.exit('No active session. Run: bench.sh start-all')
print(p.read_text())
"
}

bench_current_field() {
  local project_dir="$1"
  local field="$2"
  bench_current_json "$project_dir" | python3 -c "
import json, sys
d = json.load(sys.stdin)
v = d.get('$field', '')
if not v:
    sys.exit(f'missing $field in current_session.json')
print(v)
"
}

bench_task_session_file() {
  local project_dir="$1"
  local task_id="$2"
  echo "${project_dir}/.oracle/session_${task_id}.json"
}

bench_active_session_id() {
  local project_dir="$1"
  local task_id
  task_id="$(bench_current_field "$project_dir" active_task_id)"
  python3 -c "
import json
d = json.load(open('$(bench_task_session_file "$project_dir" "$task_id")'))
print(d['session_id'])
"
}

bench_usage() {
  cat >&2 <<EOF
TUI-Bench — one entry point, no manual IDs.

Usage:
  bench.sh [-C project-dir] <command>

Commands:
  setup                 Optional: pip install bench-oracle
  list                  List tasks
  build [--rebuild]     Build Docker image
  start-all [--human]   Suite: fingerprint all tasks, start T01
  start T01 [--human]   Start a single task only
  next [--human]        After verify, go to next task
  current               Show active task (e.g. T02 — 2/4)
  task                  Agent task JSON for active task
  use                   agent-tui use (active session)
  verify [--human]      One-line PASS/FAIL
  stop                  Stop and clear session state

State in .oracle/:
  current_session.json       suite progress + active task
  session_T01.json …         per-task docker/tmux IDs + description
  T01_fingerprints.json …    per-task fingerprints only

Typical suite flow:
  export BENCH_PROJECT=/path/to/Finshed/P01-aptui
  bench.sh start-all --human
  bench.sh task
  # ← English-only text/Agent English-only text TUI（agent-tui English-only text bench.sh use），English-only text ORACLE_GUIDE.md §3
  bench.sh verify --human
  bench.sh next --human
  … repeat …
  bench.sh stop
EOF
}
