#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Supermarket"
check_screen "$FP_FILE_CONTENT"
check_screen "85.14"
check_screen "Alice"
check_screen "Food"

oracle_msg "T01 PASS"
