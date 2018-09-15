#!/bin/bash -ex
cd `dirname $0`

export WORKSPACE=`pwd`

if [ -z ${WINEPREFIX} ]; then
  export WINEBASE=$HOME/.wine
else
  export WINEBASE=$WINEPREFIX
fi

export WINEARCH=win32
export WINEPREFIX=$WORKSPACE/target/.wine/

rm -fR $WINEPREFIX
mkdir -p $WINEPREFIX/dosdevices
cp -P --reflink=auto $WINEBASE/*.reg $WINEBASE/.update-timestamp $WINEPREFIX/
ln -s $WINEBASE/dosdevices/c: $WINEPREFIX/dosdevices/c:
ln -s $WINEBASE/dosdevices/z: $WINEPREFIX/dosdevices/z:
ln -s $WORKSPACE $WINEPREFIX/dosdevices/w:

export WIXSHARP_WIXDIR="W:/target/wixsharp/Wix_bin/bin"
export WIXSHARP_DIR="W:/target/wixsharp"

wget -P target https://github.com/oleg-shilo/wixsharp/releases/download/v1.9.1.0/WixSharp.1.9.1.0.7z
7z x -y -otarget/wixsharp/ target/WixSharp.1.9.1.0.7z

wine target/wixsharp/cscs.exe wixcs/setup.cs
chmod 644 target/*.msi
