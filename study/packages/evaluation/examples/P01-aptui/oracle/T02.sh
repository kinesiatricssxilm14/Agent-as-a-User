#!/usr/bin/env bash
set -euo pipefail
source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"
load_fingerprints
check_shell "dpkg -l curl 2>&1 | grep -c '^ii' || true" "0"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"
oracle_msg "T02 PASS"
