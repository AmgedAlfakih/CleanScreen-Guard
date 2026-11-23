@echo off
cd /d "%~dp0"
if not exist ".venv\Scripts\python.exe" (
    echo Virtual environment not found. Running setup_and_run.bat instead...
    call setup_and_run.bat
    exit /b
)
start "" /b .\.venv\Scripts\pythonw.exe app.py
