#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

# check_shell "test -f /bench/data/src/renamed.txt && test ! -f /bench/data/src/file1.txt && echo ok" "ok"
ORACLE_SEARCH_ROOTS="/bench/data/src/renamed.txt" check_filesystem_content "file1 $FP_FILE_CONTENT"
check_shell "test ! -f /bench/data/src/file1.txt && echo ok" "ok"

oracle_msg "T04 PASS"
