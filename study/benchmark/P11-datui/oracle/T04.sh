#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "salary"
check_screen "uniform"
check_screen "0.691"
check_screen "0.878"
check_screen "0.384"
check_screen "1.80"

oracle_msg "T04 PASS"