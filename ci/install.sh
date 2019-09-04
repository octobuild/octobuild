#!/bin/bash -ex

if [ "${CLIPPY}" == "true" ]; then
  rustup component add clippy
elif [ "${RUSTFMT}" == "true" ]; then
  rustup component add rustfmt
else
  if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
    choco install nuget.commandline
    nuget install WiX
  elif [ "${TRAVIS_OS_NAME}" = "linux" ]; then
    # Fix this when https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#install-upgrade is stabilized
    cargo install cargo-deb || true
  fi
fi

if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
  choco install capnproto
fi
