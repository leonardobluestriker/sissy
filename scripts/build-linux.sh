#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
# Development build, tested on Debian 13 x86_64. Requires a graphical session.
cargo build --locked
mkdir -p dist/linux-x86_64
cp target/debug/sissy dist/linux-x86_64/sissy
cp README.md dist/linux-x86_64/README.md
tar -czf dist/Sissy-linux-x86_64-debug.tar.gz -C dist linux-x86_64
