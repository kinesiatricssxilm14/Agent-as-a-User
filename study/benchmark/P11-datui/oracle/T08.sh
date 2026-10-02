#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-1732"
check_screen "EMP-4748"
check_screen "EMP-1726"
check_screen "EMP-3965"
check_screen "EMP-1130"

oracle_msg "T08 PASS"