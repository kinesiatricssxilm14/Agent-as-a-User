#!/usr/bin/env bash
# oracle/Txx.sh — all verification logic for this task lives here.
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

# Resolve evaluation/scripts/oracle_lib.sh
if [[ -n "${BENCH_ORACLE_LIB:-}" ]]; then
  # shellcheck source=/dev/null
  source "$BENCH_ORACLE_LIB"
else
  for candidate in \
    "${ORACLE_PROJECT_DIR}/../../evaluation/scripts/oracle_lib.sh" \
    "${ORACLE_PROJECT_DIR}/../../../evaluation/scripts/oracle_lib.sh" \
    "/path/to/anonymous-artifact"; do
    if [[ -f "$candidate" ]]; then
      # shellcheck source=/dev/null
      source "$candidate"
      break
    fi
  done
fi

load_fingerprints

# --- checks below (example: filesystem + auxiliary shell) ---

check_shell "dpkg -l tree 2>/dev/null | grep -c '^ii'" "1"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"

oracle_msg "T01 PASS"
