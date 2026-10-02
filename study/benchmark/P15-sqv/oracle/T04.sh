#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "982"
check_screen "William Hoffman"
check_screen "uvasquez@example.org"
check_screen "24"
check_screen "Marketing"

oracle_msg "T04 PASS"
