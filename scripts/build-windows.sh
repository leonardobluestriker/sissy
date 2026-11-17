#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-gnu
rustup target add "$target"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --release --locked --target "$target"
mkdir -p dist/windows-x86_64
cp target/$target/release/sissy.exe dist/windows-x86_64/Sissy.exe
x86_64-w64-mingw32-strip dist/windows-x86_64/Sissy.exe
# This is an experimental scanner. Finder actions are macOS-only.
echo "Gerado: dist/windows-x86_64/Sissy.exe (experimental, sem assinatura)"
