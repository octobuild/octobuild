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

github-release release --tag $TAGNAME --draft
	
for i in target/*.msi target/*.nupkg; do
	github-release upload --tag $TAGNAME --file $i --name `basename $i`
done

for i in target/*.nupkg; do
	nuget push $i -Source "$NUGET_REPO" -ApiKey "$NUGET_TOKEN"
done
