/*
sudo apt install mingw-w64
sudo apt install wine-1.8
sudo apt install p7zip-full
sudo apt install mono-devel

export WINEARCH=win32
export WINEPREFIX=/home/bozaro/.wine-i686/

winetricks dotnet40
wine reg add "HKLM\\Software\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\S-1-5-21-0-0-0-1000"
*/
parallel 'Linux': {
  node ('linux') {
    stage 'Linux: Checkout'
    checkout scm
    sh 'git reset --hard'
    sh 'git clean -ffdx'

    stage 'Linux: Prepare rust'
    withRustEnv {
      sh 'rustup update'
      sh 'rustup override add stable'
    }

    stage 'Linux: Test'
    withRustEnv {
      sh 'cargo test'
    }

    stage 'Linux: Build'
    withRustEnv {
      sh 'cargo build --release --target x86_64-unknown-linux-gnu'
    }
  }
},
'Win64': {
  node ('linux') {
    stage 'Win64: Checkout'
    checkout scm
    sh 'git reset --hard'
    sh 'git clean -ffdx'

    stage 'Win64: Prepare rust'
    withRustEnv {
      sh 'rustup update'
      sh 'rustup override add stable'
      sh 'rustup target add x86_64-pc-windows-gnu'
    }

    stage 'Win64: Build'
    withRustEnv {
      sh 'cargo build --release --target x86_64-pc-windows-gnu'
    }

    stage 'Win64: Installer'
    sh '7z x -y -otarget/wixsharp/ .jenkins/distrib/WixSharp.1.0.35.0.7z'
    withEnv([
      'WIXSHARP_DIR=Z:$WORKSPACE/target/wixsharp',
      'WIXSHARP_WIXDIR=Z:$WORKSPACE/target/wixsharp/Wix_bin/bin',
    ]) {
      sh '''
env | sort
export WORKSPACE=`pwd`
export WIXSHARP_DIR=Z:$WORKSPACE/target/wixsharp
export WIXSHARP_WIXDIR=Z:$WORKSPACE/target/wixsharp/Wix_bin/bin
env | sort
wine target/wixsharp/cscs.exe wixcs/setup.cs
'''
    }
    archive 'target/*.msi'
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
