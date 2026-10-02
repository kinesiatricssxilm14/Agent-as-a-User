#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "requests"
check_screen "Flask"
check_screen "pip"
check_screen "apt"
check_screen "setuptools"
check_screen_absent "ca-certificates"
check_screen_absent "python3-venv"
check_screen_absent "tree"

oracle_msg "T01 PASS"
