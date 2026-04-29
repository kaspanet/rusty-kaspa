@echo off
REM Kaspa Node Auto-Start Script (Portable Version)

title Kaspa Node - Auto-Restart Script

REM Change to the directory where this script is located
cd /d "%~dp0"

:xxx
echo Starting Kaspa Node (kaspad.exe)...
echo Directory: %cd%
echo.

echo To stop, press Ctrl+C and then 'Y' when prompted.
echo.

REM Run kaspad from the same folder
kaspad.exe --utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110 

echo.
echo Kaspa Node process exited. Restarting in 5 seconds...
choice /C SR /N /T 5 /D R >nul
if errorlevel 2 goto xxx

echo Stopping by user request.
goto :eof