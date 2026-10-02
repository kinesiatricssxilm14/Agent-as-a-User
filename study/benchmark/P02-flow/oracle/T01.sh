#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Polish focused column styling"
check_screen "Subtle focus color; readable defaults."
check_screen "$FP_SCREEN"

oracle_msg "T01 PASS"
