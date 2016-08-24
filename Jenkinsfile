/*
sudo apt install mingw-w64
sudo apt install wine-1.8
sudo apt install p7zip-full
sudo apt install mono-devel
sudo apt install osslsigncode
sudo apt install nuget
sudo apt install golang
sudo apt install capnproto

export WINEARCH=win32
export WINEPREFIX=$HOME/.wine-i686/

winetricks dotnet40
wine reg add "HKLM\\Software\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\S-1-5-21-0-0-0-1000"
*/
rustVersion = "1.11.0"

def TAG_NAME = binding.variables.get("TAG_NAME")
if (TAG_NAME != null) {
  echo "Build tag: $TAG_NAME"
}

stage 'Build'
parallel 'Linux': {
  node ('linux') {
    //stage 'Linux: Checkout'
    checkout scm
    sh 'git reset --hard'
    sh 'git clean -ffdx'

    //stage 'Linux: Prepare rust'
    withRustEnv {
      sh "rustup self update"
      sh "rustup toolchain install $rustVersion"
      sh "rustup override add $rustVersion"
    }

    //stage 'Linux: Test'
    withRustEnv {
      sh 'cargo test'
    }

    //stage 'Linux: Build'
    withRustEnv {
      sh 'cargo build --release --target x86_64-unknown-linux-gnu'
    }

    //stage 'Linux: Installer'
    sh '''#!/bin/bash
# Create package
. target/release/version.sh
DATE=`date -R`

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
'''
    archive 'target/*.deb'
    archive 'target/*.dsc'
    archive 'target/*.tar.gz'
    archive 'target/*.changes'
  }
},
'Win32': windowsBuild('Win32', 'i686'),
'Win64': windowsBuild('Win64', 'x86_64')

def windowsBuild(String stageName, String arch) {
  return {
    node ('linux') {
      //stage "$stageName: Checkout"
      checkout scm
      sh "git reset --hard"
      sh "git clean -ffdx"

      //stage "$stageName: Prepare rust"
      withRustEnv {
        sh "rustup self update"
        sh "rustup toolchain install $rustVersion"
        sh "rustup override add $rustVersion"
        sh "rustup target add $arch-pc-windows-gnu"
      }

      //stage "$stageName: Build"
      withRustEnv {
        sh "cargo build --release --target $arch-pc-windows-gnu"
      }
      withCredentials([[$class: 'FileBinding', credentialsId: '54b693ef-b304-4d3d-a53b-6efd64dd76f4', variable: 'PEM_FILE']]) {
        sh """
for i in target/$arch-pc-windows-gnu/release/*.exe; do
  osslsigncode sign -certs "\$PEM_FILE" -key "\$PEM_FILE" -in \$i -h sha256 -t http://timestamp.verisign.com/scripts/timstamp.dll -out \$i.signed && mv \$i.signed \$i
done
"""
      }

      //stage "$stageName: Installer"
      sh "7z x -y -otarget/wixsharp/ .jenkins/distrib/WixSharp.1.0.35.0.7z"
      withEnv([
        'WIXSHARP_DIR=Z:$WORKSPACE/target/wixsharp',
        'WIXSHARP_WIXDIR=Z:$WORKSPACE/target/wixsharp/Wix_bin/bin',
      ]) {
        sh """
export WORKSPACE="`pwd`"
export WIXSHARP_DIR="Z:\$WORKSPACE/target/wixsharp"
export WIXSHARP_WIXDIR="Z:\$WORKSPACE/target/wixsharp/Wix_bin/bin"
wine target/wixsharp/cscs.exe wixcs/setup.cs
"""
      }
      withCredentials([[$class: 'FileBinding', credentialsId: '54b693ef-b304-4d3d-a53b-6efd64dd76f4', variable: 'PEM_FILE']]) {
        sh """
for i in target/*.msi; do
  osslsigncode sign -certs "\$PEM_FILE" -key "\$PEM_FILE" -in \$i -h sha256 -t http://timestamp.verisign.com/scripts/timstamp.dll -out \$i.signed && mv \$i.signed \$i
done
"""
      }
      archive "target/*.msi"
    }
  }
}

if (TAG_NAME != null) {
  node ('linux') {
    checkout scm
    sh "git reset --hard"
    sh "git clean -ffdx"
    unarchive mapping: ["target/" : "."]

    stage "Publish: github"
    withEnv([
      "TAG_NAME=$TAG_NAME",
      "GITHUB_USER=bozaro",
      "GITHUB_REPO=octobuild",
    ]) {
      withCredentials([[$class: 'StringBinding', credentialsId: '49bf22be-f4d4-4a75-855a-b0e56e357f1c', variable: 'GITHUB_TOKEN']]) {
        sh """
export GOPATH="\$PWD/target/golang"
export PATH="\$GOPATH/bin:\$PATH"
mkdir -p "\$GOPATH"

go get github.com/aktau/github-release

github-release info --tag $TAG_NAME || github-release release --tag $TAG_NAME --draft
for i in target/*.msi target/*.deb; do
  github-release upload --tag $TAG_NAME --file \$i --name `basename \$i`
done
"""
      }
    }

    stage "Publish: dist.bozaro.ru"
    sshagent(credentials: ['0d1e35cd-a719-4ab9-afed-fb5d9c8ff9af']) {
      sh """
scp -B -o StrictHostKeyChecking=no target/*.msi deploy@dist.bozaro.ru:dist.bozaro.ru/htdocs/windows/
scp -B -o StrictHostKeyChecking=no target/*.dsc target/*.tar.gz target/*.deb target/*.changes deploy@dist.bozaro.ru:incoming/
"""
    }

    stage "Publish: nuget"
    withCredentials([[$class: 'UsernamePasswordMultiBinding', credentialsId: '133e2d6f-04ac-4d18-8505-3025dd652355', passwordVariable: 'NUGET_TOKEN', usernameVariable: 'NUGET_REPO']]) {
        sh """
nuget pack choco/octobuild.nuspec -OutputDirectory target -Properties version=$TAG_NAME

for i in target/*.nupkg; do
  nuget push \$i -Source "\$NUGET_REPO" -ApiKey "\$NUGET_TOKEN"
done
"""
    }
  }
}

void withRustEnv(List envVars = [], def body) {
  List jobEnv = [
    'PATH+RUST=$HOME/.cargo/bin'
  ]

  // Add any additional environment variables.
  jobEnv.addAll(envVars)

  // Invoke the body closure we're passed within the environment we've created.
  withEnv(jobEnv) {
    body.call()
  }
}
