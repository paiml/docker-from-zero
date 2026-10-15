#!/usr/bin/env bash
# S4: 100% line and function coverage from real execution tracing (main.rs is a 3-line shim, excluded).
set -euo pipefail
cargo llvm-cov --ignore-filename-regex 'main\.rs' --fail-under-lines 100 --fail-under-functions 100
