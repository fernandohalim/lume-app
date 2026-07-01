#!/usr/bin/env bash
# lume build launcher (macOS / Linux). Reads an optional proxy from .env.
# Usage:
#   bash build.sh               full build + installers (.dmg/.app on macOS)
#   bash build.sh --no-bundle   just the release binary
set -e
cd "$(dirname "$0")"

if [ -f .env ]; then
  proxy="$(grep -E '^LUME_HTTP_PROXY=' .env | tail -n1 | cut -d= -f2-)"
  if [ -n "$proxy" ]; then
    export HTTP_PROXY="$proxy" HTTPS_PROXY="$proxy" NO_PROXY="localhost,127.0.0.1"
    echo "[build.sh] using proxy $proxy"
  fi
fi

npm run tauri -- build "$@"
