cd %~dp0
cargo build --release && %WIXSHARP_DIR%\cscs.exe wixcs\setup.cs
