@echo off
rem Release build: standalone exe + NSIS installer in src-tauri\target\release.
rem vcvars.cmd finds the MSVC environment, whichever Visual Studio is installed.
rem Keep this file pure ASCII - non-ASCII bytes desync the cmd parser.
cd /d %~dp0
rem package.json owns the version; copy it into tauri.conf.json and Cargo.toml
rem BEFORE the build, because `tauri build` reads its config at startup.
call node scripts\set-version.mjs || exit /b 1
call "%~dp0vcvars.cmd" || exit /b 1
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
call npx tauri build %*
