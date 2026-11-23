@echo off
cd /d "%~dp0"
echo ====================================================
echo   Building CleanScreenGuard.exe (PyInstaller)
echo ====================================================
echo.

if not exist ".venv\Scripts\python.exe" (
    echo [ERROR] Please run setup_and_run.bat at least once before building the .exe!
    pause
    exit /b 1
)

echo [1/3] Installing / checking PyInstaller...
.\.venv\Scripts\python.exe -m pip install --upgrade pyinstaller
if errorlevel 1 (
    echo [ERROR] Failed to install PyInstaller.
    pause
    exit /b 1
)

echo.
echo [2/3] Packaging CleanScreenGuard.exe with your custom icon ^& Windows metadata...
if exist "build" rmdir /s /q "build"
if exist "dist\CleanScreenGuard" rmdir /s /q "dist\CleanScreenGuard"

.\.venv\Scripts\pyinstaller.exe ^
  --noconfirm ^
  --clean ^
  --windowed ^
  --onedir ^
  --name "CleanScreenGuard" ^
  --icon "assets\app_icon.ico" ^
  --version-file "version_info.txt" ^
  --add-data "assets;assets" ^
  --collect-all onnxruntime ^
  --collect-data nudenet ^
  --collect-all dxcam ^
  --collect-all comtypes ^
  --exclude-module PySide6.QtWebEngineCore ^
  --exclude-module PySide6.QtWebEngineWidgets ^
  --exclude-module PySide6.Qt3DCore ^
  --exclude-module PySide6.Qt3DRender ^
  --exclude-module PySide6.QtMultimedia ^
  --exclude-module PySide6.QtCharts ^
  --exclude-module PySide6.QtQml ^
  --exclude-module PySide6.QtQuick ^
  --exclude-module tkinter ^
  --exclude-module matplotlib ^
  --exclude-module scipy ^
  app.py

if errorlevel 1 (
    echo.
    echo [ERROR] Build failed! Check the message above.
    pause
    exit /b 1
)

echo.
echo [3/3] Build complete: dist\CleanScreenGuard\CleanScreenGuard.exe
echo Next step: Double-click build_installer.bat to create CleanScreenGuard_Setup_1.0.0.exe!
pause
