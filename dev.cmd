@echo off
REM ============================================================
REM  lume dev launcher (corporate Windows PC)
REM  Wires up the three things a bare `npm run tauri dev` lacks:
REM    1. cargo on PATH (rustup install never set it)
REM    2. portable MSVC compiler/linker env (no admin install)
REM    3. the corporate proxy, with localhost bypassed
REM  Run from PowerShell with:  .\dev.cmd
REM ============================================================

call "C:\Users\rs-fhalim\msvc\setup_x64.bat"

set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM reqwest (Spotify API calls) only reads these env vars, not the WinINET proxy.
set "HTTP_PROXY=http://gps:8080"
set "HTTPS_PROXY=http://gps:8080"
REM Keep the OAuth loopback (127.0.0.1:8888) and the Vite dev server off the proxy.
set "NO_PROXY=localhost,127.0.0.1"

cd /d "%~dp0"
npm run tauri dev
