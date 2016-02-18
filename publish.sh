#!/bin/bash
#
# This script require:
*
#  * https://github.com/aktau/github-release
#

set -ex
cd `dirname $0`

export GITHUB_REPO=octobuild

if [ "$1" != "" ]; then
	TAGNAME=$1
fi

if [ "$TAGNAME" == "" ]; then
	echo "Tag name is not defined"
	exit 1
fi

function upload {
    scp -B -o StrictHostKeyChecking=no $@ deploy@dist.bozaro.ru:incoming/
}

scp -B -o StrictHostKeyChecking=no target/*.msi dist.bozaro.ru:htdocs/windows/

github-release info --tag $TAGNAME || github-release release --tag $TAGNAME --draft

for i in target/*.msi target/*.deb; do
	github-release upload --tag $TAGNAME --file $i --name `basename $i`
done

upload target/*.dsc target/*.tar.gz target/*.deb
upload target/*.changes

#for i in target/*.nupkg; do
#	nuget push $i -Source "$NUGET_REPO" -ApiKey "$NUGET_TOKEN"
#done
