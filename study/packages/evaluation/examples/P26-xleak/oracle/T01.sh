#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

if [[ -n "${BENCH_ORACLE_LIB:-}" ]]; then
  # shellcheck source=/dev/null
  source "$BENCH_ORACLE_LIB"
else
  for candidate in \
    "${ORACLE_PROJECT_DIR}/../../evaluation/scripts/oracle_lib.sh" \
    "${ORACLE_PROJECT_DIR}/../../../evaluation/scripts/oracle_lib.sh"; do
    if [[ -f "$candidate" ]]; then
      # shellcheck source=/dev/null
      source "$candidate"
      break
    fi
  done
fi

load_fingerprints

# expect: shows Alice Engineering
check_screen "$FP_SCREEN"

oracle_msg "T01 PASS"
