#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

export ORACLE_SEARCH_ROOTS="/bench/data/result.txt"
check_filesystem_content "192.168.1.30 - GET /api/orders 200 0.14s"
check_filesystem_content "192.168.1.24 - DELETE /api/login 404 0.32s"
check_filesystem_content "192.168.1.25 - POST /api/orders 200 0.4s"
check_filesystem_content "192.168.1.22 - GET /api/orders 200 0.09s"
check_filesystem_content "192.168.1.17 - GET /api/checkout 200 0.83s"
check_filesystem_content "$FP_SCREEN"

oracle_msg "T06 PASS"
