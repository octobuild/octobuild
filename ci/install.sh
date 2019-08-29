#!/bin/bash -e

if [ "${CLIPPY}" == "true" ]; then
  rustup component add clippy
fi

if [ "${RUSTFMT}" == "true" ]; then
  rustup component add rustfmt
fi

if [ "${TRAVIS_OS_NAME}" = "windows" ]; then
  choco install capnproto
fi
