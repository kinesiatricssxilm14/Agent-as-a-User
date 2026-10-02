#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "bench-nginx"
check_screen "nginx:alpine"
check_screen "bench-redis"
check_screen "redis:alpine"
check_screen "bench-mysql"
check_screen "mysql:8.0"

oracle_msg "T01 PASS"
