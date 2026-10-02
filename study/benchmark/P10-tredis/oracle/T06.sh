#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

# Active subscriptions from seed/init.sh (SUBSCRIBE + background client)
check_screen "bench:news"
check_screen "bench:alerts"
check_screen "bench:metrics"
check_screen "bench:events"
check_screen "bench:notify"
check_screen "bench:$FP_SCREEN"

oracle_msg "T06 PASS"
