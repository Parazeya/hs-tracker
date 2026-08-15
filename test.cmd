@echo off
rem The Rust tests with the MSVC environment loaded, the same way tauri-dev.cmd
rem does it: rustc cannot link without it.
rem Keep this file pure ASCII and CRLF - cmd refuses a batch file with LF only.
cd /d %~dp0
call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cargo test --manifest-path src-tauri\Cargo.toml %*
