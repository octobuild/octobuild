#!/bin/bash -e

if [ "${CLIPPY}" == "true" ]; then
  cargo clippy --all-targets
  exit
fi

if [ "${RUSTFMT}" == "true" ]; then
  cargo fmt --all -- --check
  exit
fi

cargo test --verbose
