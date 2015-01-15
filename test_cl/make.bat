@set PATH=%~dp0..\target;%PATH%
@call "C:\Program Files (x86)\Microsoft Visual Studio 12.0\Common7\Tools\vsvars32.bat"
cargo build && nmake clean all