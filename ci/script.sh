#!/bin/bash -ex

if [ "${CLIPPY}" == "true" ]; then
  cargo clippy --all-targets -- -D warnings
  exit
fi

if [ "${RUSTFMT}" == "true" ]; then
  cargo fmt --all -- --check
  exit
fi

cargo test --all-targets

if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
  cargo wix --output target/deploy/ --nocapture --bin-path WiX.3.11.1/tools
elif [ "${TRAVIS_OS_NAME}" = "linux" ]; then
  cargo deb --output target/deploy/
fi
