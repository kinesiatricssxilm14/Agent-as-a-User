#!/bin/bash
# P02 flow seed — install board directory from seed to target path
set -e
if [ -d /bench/data/board/cols ]; then
  : # volume mount or image already provisioned
elif [ -d /bench/boards/demo ]; then
  mkdir -p /bench/data
  rm -rf /bench/data/board
  cp -r /bench/boards/demo /bench/data/board
else
  echo "flow: no board seed at /bench/boards/demo or /bench/data/board" >&2
  exit 1
fi
