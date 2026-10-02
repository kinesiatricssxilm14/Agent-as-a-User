#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "192.168.1.10 - PUT /api/users 404 0.04s"
check_screen "192.168.1.18 - PUT /api/orders 404 0.03s"     
check_screen "192.168.1.21 - PUT /api/items 404 0.01s"
check_screen "192.168.1.14 - DELETE /api/items 404 0.02s"
check_screen "192.168.1.24 - DELETE /api/login 404 0.32s"


oracle_msg "T02 PASS"
