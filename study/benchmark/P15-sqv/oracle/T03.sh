#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints


check_screen "Joel Steele"
check_screen "894"
check_screen "frank45@example.com"
check_screen "31"
check_screen "Design"

oracle_msg "T03 PASS"
