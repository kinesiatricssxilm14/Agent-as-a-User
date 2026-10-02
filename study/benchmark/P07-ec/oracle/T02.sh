#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

export ORACLE_SEARCH_ROOTS="/bench/data/repo/conflict.py"
check_filesystem_content "line1
feature version
line3
line4
line5-new
${FP_SCREEN}"

oracle_msg "T02 PASS"
