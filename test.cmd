@echo off
rem The Rust tests with the MSVC environment loaded, the same way tauri-dev.cmd
rem does it: rustc cannot link without it. See vcvars.cmd.
rem Keep this file pure ASCII and CRLF - cmd refuses a batch file with LF only.
cd /d %~dp0
call "%~dp0vcvars.cmd" || exit /b 1
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cargo test --manifest-path src-tauri\Cargo.toml %*
