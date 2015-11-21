#!/bin/bash -ex
cd `pwd $0`
DEBROOT=target/debian

# Build system
cargo build --release

# Remove debian build directory
rm -fR DEBROOT

# Create debian build directory
CONTROL=target/release/control.txt
mkdir -p $DEBROOT/DEBIAN
mkdir -p $DEBROOT/usr/bin
cp LICENSE $DEBROOT/DEBIAN/license
cp $CONTROL $DEBROOT/DEBIAN/control
for i in xgConsole octo_clang; do
    cp target/release/$i $DEBROOT/usr/bin/$i
    strip $DEBROOT/usr/bin/$i
done
chmod -R go-w $DEBROOT

# Create package
VERSION=`grep -e "Version: " $CONTROL | awk '{print $2}'`
ARCH=`grep -e "Architecture: " $CONTROL | awk '{print $2}'`
fakeroot dpkg-deb --build $DEBROOT target/octobuild_${VERSION}_${ARCH}.deb
