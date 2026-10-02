#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "bench-data"
check_screen "local"
check_screen "/var/lib/docker/volumes/bench-data/_data"

oracle_msg "T07 PASS"
