#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "bench-mysql"
check_screen "mysql"
check_screen "8.0"
check_screen "3306"
check_screen "tcp"
check_screen "docker-entrypoint.sh mysqld"
check_screen "c3d4e5f6a1b2"

oracle_msg "T06 PASS"
