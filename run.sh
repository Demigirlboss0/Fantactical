#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

MODE="${1:-debug}"

case "$MODE" in
    release|--release|-r)
        echo "[fantactical] Building and launching (release)..."
        RUSTFLAGS="-C opt-level=2" cargo run --release
        ;;
    check|--check|-c)
        echo "[fantactical] Running tests..."
        cargo test
        ;;
    *)
        echo "[fantactical] Building and launching (debug)..."
        cargo run
        ;;
esac
