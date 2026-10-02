#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

export ORACLE_SEARCH_ROOTS="/bench/data/board/cols/todo"
check_filesystem "FLOW-2" "$FP_FILE_CONTENT"

oracle_msg "T04 PASS"
