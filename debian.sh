#!/bin/bash -ex
cd `pwd $0`

# Build system
cargo build --release

# Create package
. target/release/version.sh
DATE=`date -R`

# Check tag and version
if [ "$TAGNAME" != "" ]; then
    if [ "$TAGNAME" != "$VERSION" ]; then
	echo "Tag name is not same as version: $TAGNAME != $VERSION"
        exit 1
    fi
fi

# Copy debian config files
DEBROOT=target/octobuild
rm -fR $DEBROOT
mkdir -p $DEBROOT/
cp -r  debian $DEBROOT/

for i in $DEBROOT/debian/*; do
    sed -i -e "s/#VERSION#/$VERSION/" $i
    sed -i -e "s/#DATE#/$DATE/" $i
done

pushd $DEBROOT
dpkg-buildpackage -uc -us
popd
