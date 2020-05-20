#!/bin/bash

set -e

if [ "${CLIPPY}" == "true" ]; then
  rustup component add clippy
elif [ "${RUSTFMT}" == "true" ]; then
  rustup component add rustfmt
else
  if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
    cinst -y nuget.commandline
    nuget install WiX
    cargo install cargo-wix
  elif [ "${TRAVIS_OS_NAME}" = "linux" ]; then
    cargo install cargo-deb
  fi
fi
