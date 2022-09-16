$ErrorActionPreference = 'Stop';
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$url64      = 'https://github.com/bozaro/octobuild/releases/download/{{ version }}/octobuild-{{ version }}-x86_64.msi'

$packageArgs = @{
    packageName   = $env:ChocolateyPackageName
    unzipLocation = $toolsDir
    fileType      = 'MSI'
    url64bit      = $url64

    softwareName  = 'Octobuild'

    checksum64    = '{{ sha256 }}'
    checksumType64= 'sha256'

    silentArgs    = "/qn /norestart /l*v `"$($env:TEMP)\$($packageName).$($env:chocolateyPackageVersion).MsiInstall.log`""
    validExitCodes= @(0, 3010, 1641)
}

Install-ChocolateyPackage @packageArgs