#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "name"
check_screen "Alice"
check_screen "email"
check_screen "alice@example.com"
check_screen "age"
check_screen "30"
check_screen "note"
check_screen "$FP_SCREEN"

oracle_msg "T03 PASS"
