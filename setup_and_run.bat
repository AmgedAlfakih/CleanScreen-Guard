@echo off
cd /d "%~dp0"
echo ====================================================
echo   CleanScreen Guard - Setup & Launch
echo ====================================================
echo.

python --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Python is not installed or not added to PATH.
    echo Please install Python 3.12 from https://www.python.org/downloads/
    echo Make sure to check "Add python.exe to PATH" during installation.
    pause
    exit /b 1
)

if not exist ".venv" (
    echo Creating virtual environment...
    python -m venv .venv
)

echo Installing required libraries...
.\.venv\Scripts\python.exe -m pip install --upgrade pip
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
if errorlevel 1 (
    echo.
    echo [ERROR] Base package installation failed.
    pause
    exit /b 1
)

echo.
echo Ensuring clean Windows GPU acceleration (DirectML)...
.\.venv\Scripts\python.exe -m pip uninstall -y onnxruntime onnxruntime-directml
.\.venv\Scripts\python.exe -m pip install onnxruntime-directml

echo.
echo Setup complete! Launching CleanScreen Guard...
.\.venv\Scripts\python.exe app.py
pause
