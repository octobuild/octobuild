set WIX_ARCH=x64
cargo build --release && "%WIX%\bin\candle.exe" -arch %WIX_ARCH% octobuild.wix -o target\octobuild-%WIX_ARCH%.wixobj -nologo && "%WIX%\bin\light.exe" -sw1076 -ext WixUIExtension -o target\octobuild-%WIX_ARCH%.msi -b stage-%WIX_ARCH% -nologo target/octobuild-%WIX_ARCH%.wixobj
