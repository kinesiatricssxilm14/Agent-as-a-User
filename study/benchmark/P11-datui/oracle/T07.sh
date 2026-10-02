#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-1928"
check_screen "EMP-1930"
check_screen "EMP-2451"
check_screen "EMP-1144"
check_screen "EMP-0960"

oracle_msg "T07 PASS"