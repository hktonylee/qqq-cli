#!/bin/sh
set -eu

compatibility_docs_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
cd "$compatibility_docs_dir/.."

exec cargo test --locked --test compatibility documentation::compatibility_documentation "$@"
