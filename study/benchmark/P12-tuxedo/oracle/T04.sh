#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "(B) Fix CI pipeline +devops @laptop"
check_screen_absent "(C) Review pull requests +work @computer"

oracle_msg "T04 PASS"
