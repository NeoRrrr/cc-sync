@echo off
setlocal

cd /d "%~dp0"

if not exist "node_modules" (
    echo [cc-sync] node_modules not found, running npm install...
    call npm install
    if errorlevel 1 goto :fail
)

echo [cc-sync] building tauri package...
call npm run tauri:build
if errorlevel 1 goto :fail

exit /b 0

:fail
echo [cc-sync] command failed.
pause
exit /b 1
