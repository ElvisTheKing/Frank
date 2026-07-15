#!/bin/sh
set -eu

cargo xwin build \
    --release \
    --target x86_64-pc-windows-msvc \
    -p imagecompare-desktop

mkdir -p /workspace/dist/windows-x64
cp /workspace/target/x86_64-pc-windows-msvc/release/imagecompare-desktop.exe \
    /workspace/dist/windows-x64/imagecompare-desktop.exe
