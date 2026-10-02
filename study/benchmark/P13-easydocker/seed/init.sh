#!/bin/bash
# P13 · easydocker seed — container-side init (entrypoint → /bench/init.sh)

mkdir -p /var/run

if [ ! -S /var/run/docker.sock ]; then
  echo "[init] Starting mock Docker API server..."
  python3 /bench/mock_docker_api.py &
  sleep 1
  if [ -S /var/run/docker.sock ]; then
    echo "[init] Mock Docker API ready on /var/run/docker.sock"
  else
    echo "[init] ERROR: Mock Docker API failed to start."
    exit 1
  fi
else
  echo "[init] Docker socket already exists, skipping mock API."
fi
