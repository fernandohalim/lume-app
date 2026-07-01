@echo off
REM ============================================================
REM  lume Windows build (portable MSVC + optional proxy from .env)
REM  Usage:
REM    .\build.cmd               full build + installers (NSIS/MSI)
REM    .\build.cmd --no-bundle   just the runnable .exe (fastest, no
REM                              installer-tooling download)
REM  Output:
REM    exe:        src-tauri\target\release\lume.exe
REM    installers: src-tauri\target\release\bundle\
REM ============================================================

call "C:\Users\rs-fhalim\msvc\setup_x64.bat"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM Optional proxy from .env. Blank/absent => direct connection (home / Mac).
set "LUME_HTTP_PROXY="
if exist "%~dp0.env" for /f "usebackq eol=# tokens=1,* delims==" %%a in ("%~dp0.env") do (
  if /i "%%a"=="LUME_HTTP_PROXY" set "LUME_HTTP_PROXY=%%b"
)
if not "%LUME_HTTP_PROXY%"=="" (
  set "HTTP_PROXY=%LUME_HTTP_PROXY%"
  set "HTTPS_PROXY=%LUME_HTTP_PROXY%"
  set "NO_PROXY=localhost,127.0.0.1"
  echo [build.cmd] using proxy %LUME_HTTP_PROXY%
)

cd /d "%~dp0"
npm run tauri -- build %*
