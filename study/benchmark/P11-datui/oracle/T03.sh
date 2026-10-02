#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "EMP-2037"
check_screen "EMP-2775"
check_screen "EMP-3516"
check_screen "Roger Hess"
check_screen "Dylan Marsh"
check_screen "Angela Walker"

oracle_msg "T03 PASS"