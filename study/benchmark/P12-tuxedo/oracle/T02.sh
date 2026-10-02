#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

export ORACLE_SEARCH_ROOTS="/bench/data/todo.txt"
check_filesystem_content "$FP_FILE_CONTENT"
check_filesystem_content "@work"
check_filesystem_content "due:2029-12-12"
check_filesystem_content "(A)"

oracle_msg "T02 PASS"
