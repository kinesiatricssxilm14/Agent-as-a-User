#!/usr/bin/env bash
# LLM keyboard-agent runner using curated-study-kit packages + benchmarks.
#
# Usage:
#   ./run-agent-cn.sh --limit 20 --rebuild
#   ./run-agent.sh --limit 20 -c config.deepseek.yaml --dry-run
#
# Results go to curated-study-kit/runs/<model>-<Pxx>-<mode>-<ts>/
# (runner label = model name).
#
# Options:
#   --limit N            First N tasks across projects (default: all)
#   --from Pxx-slug      Start at this project
#   -p / --project       Only one project
#   -c / --config        Agent YAML (default: config.deepseek.yaml)
#   -o / --output        Output root (default: KIT_DIR/runs)
#   --rebuild            Rebuild Docker image on each project's first start
#   --observation-mode   plain|semantic|png|svg
#   --model NAME         Override model (also used as run-id label)
#   --max-turns N        Hard-cap agent turns (forwarded to keyboard-agent)
#   --continue-on-error  Keep going after failure (default)
#   --stop-on-error      Stop on first failure
#   --dry-run            Print planned cases only
set -euo pipefail

KIT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
[ -f "$KIT_DIR/.env" ] && source "$KIT_DIR/.env"
# Optional monorepo secrets (DEEPSEEK_API_KEY, …)
if [ -f "$KIT_DIR/../.env" ]; then
  # shellcheck source=/dev/null
  set -a && source "$KIT_DIR/../.env" && set +a
fi
# shellcheck source=bench-env.sh
source "$KIT_DIR/bench-env.sh"

BENCHMARK="$(resolve_benchmark_root)"
# Always prefer kit-vendored bench + agent config (ignore human CONFIG from .env)
BENCH="$KIT_DIR/packages/evaluation/scripts/bench.sh"
OUTPUT="${AGENT_OUTPUT:-$KIT_DIR/runs}"
# Optional lock so a second worker can own r2/r3 while an older
# r1-loop process would otherwise start a colliding suite.
if [[ -f "$OUTPUT/OWNER" ]]; then
  owner="$(tr -d '[:space:]' <"$OUTPUT/OWNER")"
  if [[ -n "${TUI_BENCH_NAME_PREFIX:-}" && -n "$owner" && "${TUI_BENCH_NAME_PREFIX}" != "$owner" ]]; then
    echo "==> skip new suite under $OUTPUT (OWNER=$owner this=${TUI_BENCH_NAME_PREFIX})"
    exit 0
  fi
fi
CONFIG="$KIT_DIR/config.deepseek.yaml"

LIMIT=0
FROM=""
ONLY=""
REBUILD=0
DRY_RUN=0
STOP_ON_ERROR=0
OBS_MODE=""
MODEL=""
MAX_TURNS=""

usage() {
  sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --limit) LIMIT="${2:?}"; shift ;;
    --from) FROM="${2:?}"; shift ;;
    -p|--project) ONLY="${2:?}"; shift ;;
    -c|--config) CONFIG="${2:?}"; shift ;;
    -o|--output) OUTPUT="${2:?}"; shift ;;
    --rebuild) REBUILD=1 ;;
    --observation-mode) OBS_MODE="${2:?}"; shift ;;
    --model) MODEL="${2:?}"; shift ;;
    --max-turns) MAX_TURNS="${2:?}"; shift ;;
    --continue-on-error) STOP_ON_ERROR=0 ;;
    --stop-on-error) STOP_ON_ERROR=1 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage 0 ;;
    *)
      echo "Unknown option: $1" >&2
      usage 1
      ;;
  esac
  shift
done

# Resolve relative config paths against kit dir
case "$CONFIG" in
  /*) ;;
  *) CONFIG="$KIT_DIR/$CONFIG" ;;
esac

if [[ ! -f "$CONFIG" ]]; then
  echo "Config not found: $CONFIG" >&2
  exit 1
fi
if [[ ! -f "$BENCH" ]]; then
  echo "bench.sh not found: $BENCH" >&2
  exit 1
fi
if [[ -z "${DEEPSEEK_API_KEY:-}${OPENAI_API_KEY:-}" ]]; then
  echo "Warning: neither DEEPSEEK_API_KEY nor OPENAI_API_KEY is set." >&2
fi

export BENCH
export BENCH_LOCALE="${BENCH_LOCALE:-cn}"
PY_BIN="$(command -v python3)"
export PATH="$(cd "$(dirname "$PY_BIN")" && pwd):$KIT_DIR:$PATH"

KA_PATH="$(python3 -c 'import keyboard_agent; print(keyboard_agent.__file__)')"
case "$KA_PATH" in
  "$KIT_DIR/packages/keyboard-agent"*) ;;
  *)
    echo "keyboard-agent is not from kit packages/: $KA_PATH" >&2
    echo "Run: FORCE_PACK=1 ./setup.sh && python3 -m pip install -e './packages/keyboard-agent[llm]'" >&2
    exit 1
    ;;
esac

MODEL_LABEL="$(python3 -c "
import yaml
from pathlib import Path
cfg = yaml.safe_load(Path(r'$CONFIG').read_text()) or {}
print((cfg.get('llm') or {}).get('model') or 'agent')
")"
if [[ -n "$MODEL" ]]; then
  MODEL_LABEL="$MODEL"
fi

# Build flat plan via Python (bash 3.2 friendly)
PLAN_FILE="$(mktemp)"
PROJECTS_FILE="$(mktemp)"
python3 - <<PY
import json
from pathlib import Path

bench = Path(r"$BENCHMARK")
limit = int("$LIMIT" or "0")
only = "$ONLY"
frm = "$FROM"
projects = sorted(
    p.name for p in bench.glob("P*-*")
    if p.is_dir() and (p / "bench.spec.json").is_file()
)
if only:
    if only not in projects:
        raise SystemExit(f"Project not found: {only}")
    projects = [only]
if frm:
    if frm not in projects:
        raise SystemExit(f"--from project not found: {frm}")
    projects = projects[projects.index(frm):]

rows = []
for proj in projects:
    spec = json.loads((bench / proj / "bench.spec.json").read_text())
    for t in spec["tasks"]:
        rows.append(f"{proj}\t{t['id']}")
        if limit and len(rows) >= limit:
            break
    if limit and len(rows) >= limit:
        break

Path(r"$PLAN_FILE").write_text("\n".join(rows) + ("\n" if rows else ""), encoding="utf-8")

# Group by project for suite runs when possible
from collections import OrderedDict
grouped = OrderedDict()
for line in rows:
    proj, task = line.split("\t", 1)
    grouped.setdefault(proj, []).append(task)
# Write: project<TAB>task,task,...
out = []
for proj, tasks in grouped.items():
    out.append(proj + "\t" + ",".join(tasks))
Path(r"$PROJECTS_FILE").write_text("\n".join(out) + ("\n" if out else ""), encoding="utf-8")
print(len(rows))
PY
TOTAL="$(wc -l <"$PLAN_FILE" | tr -d ' ')"

echo "==> curated-study-kit LLM agent"
echo "    locale:          $(resolve_bench_locale)"
echo "    benchmark:       $BENCHMARK"
echo "    bench.sh:        $BENCH"
echo "    config:          $CONFIG"
echo "    output:          $OUTPUT"
echo "    user/model:      $MODEL_LABEL"
echo "    max_turns:       ${MAX_TURNS:-"(from config)"}"
echo "    keyboard-agent:  $KA_PATH"
echo "    cases:           $TOTAL"

if [[ "$DRY_RUN" -eq 1 ]]; then
  idx=0
  while IFS="$(printf '\t')" read -r proj task; do
    [[ -n "$proj" ]] || continue
    idx=$((idx + 1))
    printf '  %02d  %s  %s\n' "$idx" "$proj" "$task"
  done <"$PLAN_FILE"
  rm -f "$PLAN_FILE" "$PROJECTS_FILE"
  exit 0
fi

mkdir -p "$OUTPUT"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
LOCALE_TAG="$(resolve_bench_locale)"
# One shared suite folder for all projects: .../P01-aptui/T01, P02-flow/T01, ...
SUITE_DIR="$OUTPUT/${MODEL_LABEL}-suite-${LOCALE_TAG}-${STAMP}"
mkdir -p "$SUITE_DIR"
SUMMARY_LOG="$SUITE_DIR/suite.log"
passed=0
failed=0
proj_idx=0

# Amend the earlier banner with the shared suite path.
echo "    suite_dir:       $SUITE_DIR"

while IFS="$(printf '\t')" read -r proj tasks_csv; do
  [[ -n "$proj" ]] || continue
  proj_idx=$((proj_idx + 1))
  # shellcheck disable=SC2206
  IFS=',' read -r -a TASK_ARR <<< "$tasks_csv"
  n_tasks=${#TASK_ARR[@]}

  # All tasks of this project?
  all_tasks="$(python3 -c "
import json
from pathlib import Path
spec=json.loads(Path(r'$BENCHMARK/$proj/bench.spec.json').read_text())
print(','.join(t['id'] for t in spec['tasks']))
")"
  run_whole=0
  if [[ "$tasks_csv" == "$all_tasks" ]]; then
    run_whole=1
  fi

  echo
  echo "=== project $proj_idx: $proj (${n_tasks} tasks) user=$MODEL_LABEL ===" | tee -a "$SUMMARY_LOG"

  if [[ "$run_whole" -eq 1 ]]; then
    cmd=(
      keyboard-agent run
      -c "$CONFIG"
      -p "$BENCHMARK/$proj"
      --bench "$BENCH"
      -o "$OUTPUT"
      --run-dir "$SUITE_DIR"
    )
    [[ "$REBUILD" -eq 1 ]] && cmd+=(--rebuild)
    [[ -n "$OBS_MODE" ]] && cmd+=(--observation-mode "$OBS_MODE")
    [[ -n "$MODEL" ]] && cmd+=(--model "$MODEL")
    [[ -n "$MAX_TURNS" ]] && cmd+=(--max-turns "$MAX_TURNS")

    TASK_LOG="$(mktemp)"
    set +e
    # Protect the PROJECTS_FILE while-read loop: agents/docker may drain stdin.
    "${cmd[@]}" >"$TASK_LOG" 2>&1 </dev/null
    rc=$?
    set -e
    cat "$TASK_LOG" | tee -a "$SUMMARY_LOG"

    # Count only this project's PASS/FAIL markers (shared summary accumulates).
    pcount=0
    fcount=0
    if grep -q '\[PASS\]' "$TASK_LOG"; then
      pcount="$(grep -c '\[PASS\]' "$TASK_LOG" || true)"
    fi
    if grep -q '\[FAIL\]' "$TASK_LOG"; then
      fcount="$(grep -c '\[FAIL\]' "$TASK_LOG" || true)"
    fi
    if [[ "${pcount:-0}" -gt 0 || "${fcount:-0}" -gt 0 ]]; then
      passed=$((passed + pcount))
      failed=$((failed + fcount))
    elif [[ "$rc" -eq 0 ]]; then
      passed=$((passed + n_tasks))
    else
      failed=$((failed + n_tasks))
    fi
    rm -f "$TASK_LOG"
    if [[ "$rc" -ne 0 && "$STOP_ON_ERROR" -eq 1 ]]; then
      rm -f "$PLAN_FILE" "$PROJECTS_FILE"
      exit "$rc"
    fi
  else
    # Partial project: run tasks one by one into the same suite dir
    for task in "${TASK_ARR[@]}"; do
      echo "--- $proj $task ---" | tee -a "$SUMMARY_LOG"
      cmd=(
        keyboard-agent run
        -c "$CONFIG"
        -p "$BENCHMARK/$proj"
        --bench "$BENCH"
        -o "$OUTPUT"
        --run-dir "$SUITE_DIR"
        --task "$task"
      )
      [[ "$REBUILD" -eq 1 ]] && cmd+=(--rebuild) && REBUILD=0
      [[ -n "$OBS_MODE" ]] && cmd+=(--observation-mode "$OBS_MODE")
      [[ -n "$MODEL" ]] && cmd+=(--model "$MODEL")
      [[ -n "$MAX_TURNS" ]] && cmd+=(--max-turns "$MAX_TURNS")

      TASK_LOG="$(mktemp)"
      set +e
      "${cmd[@]}" >"$TASK_LOG" 2>&1 </dev/null
      rc=$?
      set -e
      cat "$TASK_LOG" | tee -a "$SUMMARY_LOG"
      if grep -q '\[PASS\]' "$TASK_LOG"; then
        passed=$((passed + 1))
      elif grep -q '\[FAIL\]' "$TASK_LOG" || [[ "$rc" -ne 0 ]]; then
        failed=$((failed + 1))
        echo "FAILED: $proj $task rc=$rc" | tee -a "$SUMMARY_LOG"
        rm -f "$TASK_LOG"
        if [[ "$STOP_ON_ERROR" -eq 1 ]]; then
          rm -f "$PLAN_FILE" "$PROJECTS_FILE"
          exit 1
        fi
        continue
      else
        passed=$((passed + 1))
      fi
      rm -f "$TASK_LOG"
      REBUILD=0
    done
    REBUILD=0
  fi
done <"$PROJECTS_FILE"

rm -f "$PLAN_FILE" "$PROJECTS_FILE"

# Prefer authoritative suite summary when present
if [[ -f "$SUITE_DIR/summary.json" ]]; then
  read -r passed failed TOTAL <<<"$(python3 - <<PY
import json
from pathlib import Path
s=json.loads(Path(r"$SUITE_DIR/summary.json").read_text())
print(int(s.get("passed") or 0), int(s.get("failed") or 0))
PY
)"
fi

echo
echo "==> Done. pass≈$passed fail≈$failed / $TOTAL"
echo "    user/model: $MODEL_LABEL"
echo "    suite: $SUITE_DIR"
echo "    log:   $SUMMARY_LOG"
echo "    tip:   ls \"$SUITE_DIR\""
