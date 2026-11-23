@echo off
rem Shared environment for the Windows entry points. Called from the repo root.
set "HS_VCVARS="
for /f "delims=" %%I in ('node scripts\vcvars-path.mjs') do set "HS_VCVARS=%%I"
if not defined HS_VCVARS exit /b 1
if not exist "%HS_VCVARS%" exit /b 1
call "%HS_VCVARS%" >nul
if errorlevel 1 exit /b 1
if defined CARGO_HOME (set "PATH=%CARGO_HOME%\bin;%PATH%") else (set "PATH=%USERPROFILE%\.cargo\bin;%PATH%")
exit /b 0
