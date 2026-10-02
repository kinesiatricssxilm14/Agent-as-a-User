#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "docs_high$FP_FILE_CONTENT.jpg"
check_screen "forward_difference.md"
check_screen "production_present.md"

oracle_msg "T04 PASS"