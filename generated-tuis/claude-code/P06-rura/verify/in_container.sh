#!/usr/bin/env bash
# Run the in-container workflow verification.
#
# Usage (from the toolf/ source directory):
#   docker build -f verify/Dockerfile.verify -t toolf-verify .
#   docker run --rm -v "$PWD:/host:ro" toolf-verify bash /host/verify/in_container.sh
set -euo pipefail

apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq python3 python3-pip >/dev/null 2>&1
pip3 install --quiet --break-system-packages pyte >/dev/null 2>&1

# Generate a sample log at the documented default path.
mkdir -p /bench/data
for i in $(seq 1 50); do
  if   [ $((i % 6)) -eq 0 ]; then L=ERROR
  elif [ $((i % 4)) -eq 0 ]; then L=WARN
  else                            L=INFO
  fi
  printf '2026-08-13T09:%02d:00 %s svc=api req=%d status=%d\n' \
    "$i" "$L" "$i" "$((200 + i % 3 * 100))"
done > /bench/server.log

python3 /host/verify/container_check.py
