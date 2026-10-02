#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "alpine"
check_screen "3.19"
check_screen "7.6"
check_screen "aaa111bbb222"

check_screen "redis"
check_screen "alpine"
check_screen "33.4"
check_screen "def789abc123"

check_screen "mysql"
check_screen "8.0"
check_screen "143.1"
check_screen "789abc123def"

check_screen "nginx"
check_screen "alpine"
check_screen "42.9"
check_screen "abc123def456"


oracle_msg "T03 PASS"
