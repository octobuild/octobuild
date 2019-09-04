#!/bin/bash -ex

if [ "${CLIPPY}" == "true" ]; then
  cargo clippy --all-targets
  exit
fi

if [ "${RUSTFMT}" == "true" ]; then
  cargo fmt --all -- --check
  exit
fi

cargo test

if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
  cargo build --release
  ./wix/build-msi.sh
elif [ "${TRAVIS_OS_NAME}" = "linux" ]; then
  cargo deb --output target/deploy/
fi
