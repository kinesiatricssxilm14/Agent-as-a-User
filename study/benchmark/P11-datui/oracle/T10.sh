#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-2414"
check_screen "EMP-3004"
check_screen "EMP-3404"
check_screen "EMP-1030"
check_screen "EMP-2781"

oracle_msg "T10 PASS"