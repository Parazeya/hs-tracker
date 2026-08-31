@echo off
rem Dev run with the MSVC environment (rustc needs vcvars to link).
rem vcvars.cmd finds it, whichever Visual Studio this machine has.
rem Keep this file pure ASCII - non-ASCII bytes desync the cmd parser.
cd /d %~dp0
call "%~dp0vcvars.cmd" || exit /b 1
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
call npx tauri dev %*
