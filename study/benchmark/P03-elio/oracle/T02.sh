#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

# Fixed path: set ORACLE_SEARCH_ROOTS to a single file path and scan only that file.
# check_filesystem_content verifies content only; missing file -> not found -> fail.

# Copy to dst succeeded
ORACLE_SEARCH_ROOTS="/bench/data/dst/sample.txt" check_filesystem_content "hello world"
ORACLE_SEARCH_ROOTS="/bench/data/dst/sample.txt" check_filesystem_content "benchmark$FP_FILE_CONTENT"

# Source file unchanged
ORACLE_SEARCH_ROOTS="/bench/data/src/sample.txt" check_filesystem_content "hello world"
ORACLE_SEARCH_ROOTS="/bench/data/src/sample.txt" check_filesystem_content "benchmark$FP_FILE_CONTENT"

oracle_msg "T02 PASS"
