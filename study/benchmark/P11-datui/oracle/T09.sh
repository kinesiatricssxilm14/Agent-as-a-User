#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-4967"
check_screen "EMP-4858"
check_screen "EMP-4768"
check_screen "EMP-4430"
check_screen "EMP-3473"
check_screen "EMP-1493"
check_screen "EMP-0911"
check_screen "EMP-0682"
check_screen "EMP-0044"

oracle_msg "T09 PASS"