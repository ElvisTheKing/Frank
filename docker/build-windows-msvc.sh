#!/bin/sh
set -eu

cargo xwin build \
    --release \
    --locked \
    --target x86_64-pc-windows-msvc \
    -p imagecompare-desktop

mkdir -p /workspace/dist/windows-x64
rm -f /workspace/dist/windows-x64/imagecompare-desktop.exe
cp /workspace/target/x86_64-pc-windows-msvc/release/frank.exe \
    /workspace/dist/windows-x64/frank.exe
