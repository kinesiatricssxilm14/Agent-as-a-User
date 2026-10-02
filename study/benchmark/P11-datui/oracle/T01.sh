#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-5000"
check_screen "$FP_FILE_CONTENT"
check_screen "$FP_SCREEN"
check_screen "24"
check_screen "99.99"
check_screen "Engineering"
check_screen "US"
check_screen "benchmark_test@bench.org"

oracle_msg "T01 PASS"
