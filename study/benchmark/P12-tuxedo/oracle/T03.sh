#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "(A) Fix CI pipeline +devops @laptop"
export ORACLE_SEARCH_ROOTS="/bench/data/todo.txt"
check_filesystem_content "(A)"
check_filesystem_content "Fix CI pipeline"
check_filesystem_content "+devops"
check_filesystem_content "@laptop"

oracle_msg "T03 PASS"
