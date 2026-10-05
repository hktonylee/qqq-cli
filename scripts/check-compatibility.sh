#!/bin/sh
set -eu

compatibility_script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
cd "$compatibility_script_dir/.."

exec cargo test --locked \
    --test compatibility \
    --test snapshot \
    --test identity \
    --test images \
    --test image_storage \
    --test dependencies \
    --test cli \
    "$@"
