@echo off
setlocal
cd /d "%~dp0"

where py >nul 2>nul
if %errorlevel% equ 0 (
    py -3 gui.py
) else (
    python gui.py
)

if errorlevel 1 pause
