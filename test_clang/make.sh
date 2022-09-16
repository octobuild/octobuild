#!/bin/bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
export OCTOBUILD_CACHE=cache
export RUST_BACKTRACE=1
cargo build
make clean all
