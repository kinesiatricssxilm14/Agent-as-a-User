#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Frank Moore"
check_screen "frank.moore@ex.com"
check_screen "66"
check_screen "HR"

oracle_msg "T02 PASS"
