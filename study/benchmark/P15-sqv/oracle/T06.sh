#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_shell "sqlite3 /bench/data/bench.db 'SELECT * FROM users WHERE id = 2;'" "2|Alicer|bob@ex.com|25|$FP_SCREEN"
check_screen "Alicer"
check_screen "bob@ex.com"
check_screen "25"
check_screen "$FP_SCREEN"

oracle_msg "T06 PASS"
