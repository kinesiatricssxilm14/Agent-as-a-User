#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "score"
check_screen "age"
check_screen "salary"
check_screen "1.00"
check_screen "-0.01"
check_screen "0.00"

oracle_msg "T06 PASS"