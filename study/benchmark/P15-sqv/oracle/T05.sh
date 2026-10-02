#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "active"
check_screen "Alpha"
check_screen_absent "paused"
check_screen_absent "complete"
check_screen_absent "planning"
check_screen "13"


oracle_msg "T05 PASS"
