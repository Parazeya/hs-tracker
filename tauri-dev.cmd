@echo off
rem Dev run with the MSVC environment (rustc needs vcvars to link).
rem Keep this file pure ASCII - non-ASCII bytes desync the cmd parser.
cd /d "%~dp0"
call scripts\windows-env.cmd || exit /b 1
call npx tauri dev %*
