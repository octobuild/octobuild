#!/bin/bash -ex

version="${TRAVIS_TAG:-0.0.1}"

./WiX.*/tools/candle.exe \
  -nologo \
  -arch x64 \
  "-dProductVersion=${version}" \
  -out "target/octobuild.wixobj" \
  "wix/octobuild.wxs"

./WiX.*/tools/light.exe \
  -nologo \
  -sw1076 \
  -ext WixUIExtension \
  -spdb \
  -out "target/deploy/octobuild-${version}.msi" \
  "target/octobuild.wixobj"
