#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Write tests"
check_screen "Fix bug"
check_screen "Deploy"
check_screen "$FP_SCREEN"

oracle_msg "T04 PASS"
