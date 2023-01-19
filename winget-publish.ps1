$ErrorActionPreference = 'Stop'

$version = $args[0] -replace '.*/', "$1"

$ProgressPreference = 'SilentlyContinue'
iwr https://aka.ms/wingetcreate/latest -OutFile wingetcreate.exe
.\wingetcreate.exe update --urls "https://github.com/octobuild/octobuild/releases/download/${version}/octobuild-${version}-x86_64.msi" --version "${version}" --submit --token $args[1] "Octobuild.Octobuild"
