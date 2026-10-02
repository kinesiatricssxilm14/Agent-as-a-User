#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen_absent "requests"
check_screen_absent "Flask"
check_screen_absent "setuptools"
check_screen "ca-certificates"
check_screen "python3"
check_screen "tree"

oracle_msg "T02 PASS"
