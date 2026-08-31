@echo off
rem Load the MSVC environment, whichever Visual Studio this machine happens to
rem have. rustc shells out to link.exe and cannot find it without this.
rem
rem The three scripts that need it used to name one path each, spelled out: the
rem 2019 Build Tools under Program Files (x86). That is one of some dozens of
rem places a C++ toolset can be - four editions, every version, two program
rem folders, either on the system drive or not - so on any machine but the one
rem it was written on, `npm start` opened with a line about a command that is
rem not recognised and then a link error nobody would connect to it.
rem
rem Sourced rather than run: it is `call`ed with no setlocal, so what vcvars64
rem exports lands in the caller's environment. Callers should stop on failure -
rem   call "%~dp0vcvars.cmd" || exit /b 1
rem - because a build that goes ahead without it fails much later and much less
rem clearly.
rem
rem Two things about the way this is written, both the same thing really.
rem
rem There is not one parenthesised block in the file, only `goto` and one-line
rem `if`s. cmd finds the end of a `for (...)` or an `if (...)` by scanning for
rem the next `)`, and it is not reliably put off by one inside quotes - so the
rem moment `%ProgramFiles(x86)%` is expanded anywhere inside a block, the block
rem ends in the middle of the path. It does not fail loudly, it answers: an
rem `if exist "%ProgramFiles(x86)%\...\vswhere.exe" (...) else (...)` written
rem while working on this file reported the file missing while it sat there.
rem
rem And the two searches `pushd` into the directory they are about to read, so
rem the parenthesis never reaches a command line at all. That also lets the
rem fallback enumerate what is actually installed rather than guess version
rem numbers: this was written when the newest was 2022, on a machine carrying
rem both a 2022 and an 18. vswhere is run as `.\vswhere.exe` rather than by
rem bare name for a plainer reason - the working directory is not always on
rem the path cmd searches for an executable, and here it was not.
rem
rem Keep this file pure ASCII and CRLF - see .gitattributes.

rem Already in a developer prompt, or a caller loaded it earlier. vcvars64 is
rem not free: a second or two, and it appends to PATH every time it runs.
if defined VCINSTALLDIR exit /b 0

set "VCVARS="
set "VSROOT=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer"

rem vswhere has shipped with every Visual Studio since 2017, always in that one
rem directory whatever the installs it reports say. It is asked for the newest,
rem and `-requires` is what keeps a C#-only install out of the answer: that one
rem has no vcvars64.bat at all. `-products *` is what lets the Build Tools -
rem which is what most build machines have and all any of this needs - count.
if not exist "%VSROOT%\vswhere.exe" goto scan
pushd "%VSROOT%"
for /f "usebackq tokens=*" %%i in (`.\vswhere.exe -latest -prerelease -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2^>nul`) do if exist "%%i\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%i\VC\Auxiliary\Build\vcvars64.bat"
popd
if defined VCVARS goto load

rem No vswhere, or a toolset it declines to report. Read both program folders
rem for anything shaped like an install; first one found wins, because at this
rem point any toolset that links is a better answer than none.
:scan
call :look "%ProgramFiles%"
call :look "%ProgramFiles(x86)%"
if defined VCVARS goto load

echo.
echo   No Visual Studio C++ toolset on this machine, and rustc cannot link
echo   without one. The Build Tools alone are enough - there is no need for
echo   the IDE. Install them, tick "Desktop development with C++", and run
echo   this again:
echo.
echo     https://visualstudio.microsoft.com/downloads/
echo.
echo   If one IS installed somewhere unusual, call its vcvars64.bat before
echo   this script and it will leave the environment alone.
echo.
exit /b 1

:load
rem vcvars64 calls `vswhere.exe` by bare name itself, and on at least one
rem toolset here it does so from a directory that is not the Installer's: the
rem environment still loads and the errorlevel is still 0, but every build
rem opens with two lines saying vswhere is not a recognised command. They come
rem from Microsoft's script rather than this one, which is worth exactly the
rem four seconds it takes to read them and the twenty minutes it took the first
rem time. Putting the Installer on PATH first is what it was looking for.
if exist "%VSROOT%\vswhere.exe" set "PATH=%VSROOT%;%PATH%"
call "%VCVARS%" >nul || exit /b 1
exit /b 0

rem One program folder: <version>\<edition>, both enumerated rather than named.
:look
if defined VCVARS goto :eof
if not exist "%~1\Microsoft Visual Studio" goto :eof
pushd "%~1\Microsoft Visual Studio"
for /d %%y in (*) do for /d %%e in ("%%y\*") do if not defined VCVARS if exist "%%~fe\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fe\VC\Auxiliary\Build\vcvars64.bat"
popd
goto :eof
