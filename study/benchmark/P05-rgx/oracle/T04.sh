#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "user"
check_screen "USER"
check_screen "0"
check_screen "4"
check_screen "70"
check_screen "74"

oracle_msg "T04 PASS"
