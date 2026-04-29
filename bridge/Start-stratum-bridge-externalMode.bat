@echo off

REM Force script to run from its own directory
cd /d "%~dp0"

echo Current directory: %cd%
echo.

stratum-bridge.exe --config "config.yaml" --node-mode external --kaspad-address 127.0.0.1:16110

pause