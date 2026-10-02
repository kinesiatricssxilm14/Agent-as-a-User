#!/usr/bin/env bash
set -euo pipefail
source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"
load_fingerprints
check_answer "$FP_ANSWER"
oracle_msg "T04 PASS"
