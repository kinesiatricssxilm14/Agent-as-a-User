#!/usr/bin/env bash
set -euo pipefail
source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"
load_fingerprints
check_screen "wget"
check_screen "$FP_SCREEN"
oracle_msg "T03 PASS"
