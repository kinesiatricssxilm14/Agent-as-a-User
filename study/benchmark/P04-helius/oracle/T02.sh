#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Salary"
check_screen "800.05"
check_screen "Bob"
check_screen "2026"
check_screen "05"
check_screen "17"
check_screen "$FP_FILE_CONTENT"

oracle_msg "T02 PASS"
