#!/usr/bin/env bash
set -euo pipefail

fixture_dir=/tmp/frank-fixtures
artifact_dir=/artifacts
export XDG_RUNTIME_DIR=/tmp/frank-runtime
mkdir -p \
    "$fixture_dir" \
    "$artifact_dir" \
    "$XDG_CONFIG_HOME" \
    "$XDG_CACHE_HOME" \
    "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

cargo run --locked -p image-loader --example generate_registration_pair -- \
    "$fixture_dir/reference.jpg" "$fixture_dir/target.jpg"

lavapipe_icd=$(find /usr/share/vulkan/icd.d -name 'lvp_icd*.json' -print -quit)
if [[ -z "$lavapipe_icd" ]]; then
    echo "Lavapipe Vulkan ICD was not found" >&2
    exit 1
fi
export VK_DRIVER_FILES="$lavapipe_icd"

{
    echo "display=1440x900x24"
    echo "inspection=$EGUI_INSPECTION"
    echo "test_theme=$FRANK_TEST_THEME"
    echo "vulkan_icd=$VK_DRIVER_FILES"
    vulkaninfo --summary 2>&1 || true
} > "$artifact_dir/environment.txt"

exec xvfb-run \
    --auto-servernum \
    --server-args="-screen 0 1440x900x24 -nolisten tcp" \
    cargo run --locked -p imagecompare-desktop --features inspection -- \
    "$fixture_dir/reference.jpg" "$fixture_dir/target.jpg"
