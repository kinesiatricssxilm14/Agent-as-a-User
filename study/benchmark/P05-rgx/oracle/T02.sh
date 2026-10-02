#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "user@example.com"
check_screen "test@domain.org"
check_screen "foo@bar.co"
check_screen "USER@EXAMPLE.COM"
check_screen "admin@company.net"
check_screen "0"
check_screen "16"
check_screen "32"
check_screen "47"
check_screen "59"
check_screen "69"
check_screen "70"
check_screen "86"
check_screen "87"
check_screen "104"

oracle_msg "T02 PASS"
