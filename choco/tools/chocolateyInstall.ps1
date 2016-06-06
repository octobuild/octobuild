$packageName = "Octobuild"
$fileType = "msi"
$silentArgs = "/quiet ADDLOCAL=ALL"
$url   = "http://dist.bozaro.ru/windows/octobuild-$version$-i686.msi"
$url64 = "http://dist.bozaro.ru/windows/octobuild-$version$-x86_64.msi"

Install-ChocolateyPackage "$packageName" "$fileType" "$silentArgs" "$url" "$url64"
