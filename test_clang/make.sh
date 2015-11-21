#!/bin/bash -ex
cd `pwd $0`
export OCTOBUILD_CACHE=cache
export RUST_BACKTRACE=1
cargo build
make clean all
