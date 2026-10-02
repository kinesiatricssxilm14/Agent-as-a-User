#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "score"
check_screen "75.01"
check_screen "14.50"
check_screen "50.00"
check_screen "62.34"
check_screen "75.25"
check_screen "87.57"
check_screen "99.99"

oracle_msg "T05 PASS"