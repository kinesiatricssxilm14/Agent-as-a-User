#!/bin/bash
# P07 · ec seed
set -e

# git init /bench/data/repo
git init -b main /bench/data/repo
git -C /bench/data/repo config user.email bench@test.com
git -C /bench/data/repo config user.name Benchmark
printf 'line1\noriginal line\nline3\nline4\n' > /bench/data/repo/conflict.py && git -C /bench/data/repo add . && git -C /bench/data/repo commit -m base
git -C /bench/data/repo checkout -b feature
printf 'line1\nfeature version\nline3\nline4\nline5-new\n{{FP_SCREEN}}' > /bench/data/repo/conflict.py && git -C /bench/data/repo commit -am feat
git -C /bench/data/repo checkout main
printf 'line1\nmain version\nline3\nline4\n' > /bench/data/repo/conflict.py && git -C /bench/data/repo commit -am fix
git -C /bench/data/repo merge feature 2>/dev/null || true
