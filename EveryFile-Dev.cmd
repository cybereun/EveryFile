@echo off
setlocal
cd /d "%~dp0"
set CARGO_BUILD_JOBS=1

echo Starting EveryFile development mode...
echo Save files under src to see UI changes immediately.
call npm.cmd run tauri dev

if errorlevel 1 (
  echo.
  echo EveryFile development mode stopped with an error.
  pause
)
