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

7z x -y -otarget/wixsharp/ .jenkins/distrib/WixSharp.1.0.35.0.7z

wine target/wixsharp/cscs.exe wixcs/setup.cs
chmod 644 target/*.msi
