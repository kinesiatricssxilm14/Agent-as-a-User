#!/bin/bash
# P03 · elio seed
set -e

mkdir -p /bench/data/src /bench/data/dst
echo 'hello world benchmark{{file_content}}' > /bench/data/src/sample.txt
dd if=/dev/zero of=/bench/data/src/bigfile.bin bs=1M count=5 2>/dev/null
for i in 1 2 3; do echo file$i {{file_content}} > /bench/data/src/file${i}.txt; done
