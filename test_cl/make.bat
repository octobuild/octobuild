@set PATH=%~dp0..\target\debug;%PATH%
@set OCTOBUILD_CACHE=%~dp0cache
@call "C:\Program Files (x86)\Microsoft Visual Studio 12.0\Common7\Tools\vsvars32.bat"
cargo build && nmake clean all && echo "OK"