#!/usr/bin/env bash
# Oracle helpers for oracle/Txx.sh — source from project scripts.
set -euo pipefail

_oracle_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EVAL_ROOT="$(cd "${_oracle_lib_dir}/.." && pwd)"
export PYTHONPATH="${EVAL_ROOT}${PYTHONPATH:+:$PYTHONPATH}"

oracle_msg() { echo "[oracle] $*" >&2; }

oracle_fail() {
  oracle_msg "✗ $*"
  return 1
}

# Load fingerprint JSON into env: FP_FILE_NAME, FP_FILE_CONTENT, FP_SCREEN, FP_ANSWER, FP_RUN_ID
load_fingerprints() {
  local fp_file="${ORACLE_FP_FILE:-}"
  if [[ -z "$fp_file" || ! -f "$fp_file" ]]; then
    oracle_msg "ORACLE_FP_FILE missing; run: bench-oracle --project-dir ... gen-fingerprints ${ORACLE_TASK_ID:-T??}"
    return 1
  fi
  export FP_FILE_NAME FP_FILE_CONTENT FP_SCREEN FP_ANSWER FP_RUN_ID
  FP_FILE_NAME="$(python3 -c "import json; d=json.load(open('$fp_file')); print(d.get('file_name',''))")"
  FP_FILE_CONTENT="$(python3 -c "import json; d=json.load(open('$fp_file')); print(d.get('file_content',''))")"
  FP_SCREEN="$(python3 -c "import json; d=json.load(open('$fp_file')); print(d.get('screen',''))")"
  FP_ANSWER="$(python3 -c "import json; d=json.load(open('$fp_file')); print(d.get('answer',''))")"
  FP_RUN_ID="$(python3 -c "import json; d=json.load(open('$fp_file')); print(d.get('run_id',''))")"
}

check_filesystem() {
  local name_fp="${1:-}"
  local content_fp="${2:-}"
  local extra=()
  if [[ -n "${3:-}" ]]; then
    case "$3" in
      name-only|name) extra+=(--name-only) ;;
      content-only|content) extra+=(--content-only) ;;
      *) oracle_fail "check_filesystem: unknown mode '$3' (use name-only or content-only)" ;;
    esac
  fi
  python3 -m oracle.cli check filesystem \
    --name-fp "$name_fp" \
    --content-fp "$content_fp" \
    ${extra+"${extra[@]}"} \
    ${ORACLE_SEARCH_ROOTS:+--roots "$ORACLE_SEARCH_ROOTS"} \
    || oracle_fail "check_filesystem \"$name_fp\" \"$content_fp\" ${3:-}"
}

# Only match filename token (ignore content).
check_filesystem_name() {
  check_filesystem "$1" "" name-only
}

# Only match file content token (ignore filename).
check_filesystem_content() {
  check_filesystem "" "$1" content-only
}

# Optional last-frame file from keyboard-agent when TUI has already quit (e.g. q).
check_screen() {
  local pattern="${1:-${FP_SCREEN:-}}"
  local snap_args=()
  if [[ -n "${ORACLE_SCREEN_SNAPSHOT:-}" && -f "${ORACLE_SCREEN_SNAPSHOT}" ]]; then
    snap_args=(--snapshot "${ORACLE_SCREEN_SNAPSHOT}")
  fi
  python3 -m oracle.cli check screen --format plain --pattern "$pattern" "${snap_args[@]}" \
    || oracle_fail "check_screen \"$pattern\""
}

# Inverse of check_screen: pattern must NOT appear on screen.
check_screen_absent() {
  local pattern="${1:-}"
  if [[ -z "$pattern" ]]; then
    oracle_fail "check_screen_absent: pass a pattern argument"
  fi
  local snap_args=()
  if [[ -n "${ORACLE_SCREEN_SNAPSHOT:-}" && -f "${ORACLE_SCREEN_SNAPSHOT}" ]]; then
    snap_args=(--snapshot "${ORACLE_SCREEN_SNAPSHOT}")
  fi
  python3 -m oracle.cli check screen-absent --format plain --pattern "$pattern" "${snap_args[@]}" \
    || oracle_fail "check_screen_absent \"$pattern\""
}

# Semantic snapshot: text exists with optional fg/bg/style checks.
check_screen_highlight() {
  if [[ $# -lt 1 ]]; then
    oracle_fail "check_screen_highlight: Usage: --text TEXT [--fg COLOR] [--bg COLOR] [--colored] [--highlighted]"
  fi
  local snap_args=()
  if [[ -n "${ORACLE_SCREEN_SNAPSHOT:-}" && -f "${ORACLE_SCREEN_SNAPSHOT}" ]]; then
    snap_args=(--snapshot "${ORACLE_SCREEN_SNAPSHOT}")
  fi
  python3 -m oracle.cli check screen-highlight "$@" "${snap_args[@]}" \
    || oracle_fail "check_screen_highlight $*"
}

check_screen_color() {
  local fg="${1:-}"
  local bg="${2:-}"
  if [[ -z "$fg" && -z "$bg" ]]; then
    oracle_fail "check_screen_color: pass at least one color"
  fi
  local args=(check screen-color)
  [[ -n "$fg" ]] && args+=(--fg "$fg")
  [[ -n "$bg" ]] && args+=(--bg "$bg")
  if [[ -n "${ORACLE_SCREEN_SNAPSHOT:-}" && -f "${ORACLE_SCREEN_SNAPSHOT}" ]]; then
    args+=(--snapshot "${ORACLE_SCREEN_SNAPSHOT}")
  fi
  python3 -m oracle.cli "${args[@]}" \
    || oracle_fail "check_screen_color \"$fg\" \"$bg\""
}

check_answer() {
  local expected="${1:-${FP_ANSWER:-}}"
  python3 -m oracle.cli check answer --expected "$expected" \
    || oracle_fail "check_answer \"$expected\""
}

check_shell() {
  local cmd="$1"
  local expect="${2:-}"
  python3 -m oracle.cli check shell --cmd "$cmd" --expect "$expect" \
    || oracle_fail "check_shell \"$cmd\" \"$expect\""
}
