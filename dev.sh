#!/usr/bin/env bash
# lume dev launcher (macOS / Linux). Reads an optional proxy from .env
# (LUME_HTTP_PROXY); at home this is blank, so it just runs tauri dev.
# Prereqs: Rust (rustup), Node, and on macOS the Xcode Command Line Tools.
# Usage:  bash dev.sh   (or: chmod +x dev.sh && ./dev.sh)
set -e
cd "$(dirname "$0")"

if [ -f .env ]; then
  proxy="$(grep -E '^LUME_HTTP_PROXY=' .env | tail -n1 | cut -d= -f2-)"
  if [ -n "$proxy" ]; then
    export HTTP_PROXY="$proxy" HTTPS_PROXY="$proxy" NO_PROXY="localhost,127.0.0.1"
    echo "[dev.sh] using proxy $proxy"
  fi
fi

npm run tauri dev
