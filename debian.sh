#!/bin/bash -ex
cd `pwd $0`

# Build system
cargo build --release

# Create package
. target/release/version.sh
DATE=`date -R`

# Copy debian config files
DEBROOT=target/octobuild-${VERSION}
rm -fR $DEBROOT
mkdir -p $DEBROOT/
cp -r  debian $DEBROOT/

for i in $DEBROOT/debian/*; do
    sed -i -e "s/#VERSION#/$VERSION/" $i
    sed -i -e "s/#DATE#/$DATE/" $i
done

pushd $DEBROOT
dpkg-buildpackage
popd
