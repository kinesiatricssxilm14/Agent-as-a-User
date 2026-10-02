#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "192.168.1.30 - GET /api/orders 200 0.14s"
check_screen "192.168.1.24 - DELETE /api/login 404 0.32s"
check_screen "192.168.1.25 - POST /api/orders 200 0.4s"
check_screen "192.168.1.22 - GET /api/orders 200 0.09s"
check_screen "192.168.1.17 - GET /api/checkout 200 0.83s"
check_screen "$FP_SCREEN"

oracle_msg "T01 PASS"
