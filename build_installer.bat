@echo off
cd /d "%~dp0"
echo ====================================================
echo   Building CleanScreen Guard Setup.exe (Inno Setup)
echo ====================================================
echo.

if not exist "dist\CleanScreenGuard\CleanScreenGuard.exe" (
    echo [Info] .exe build not found yet. Running build_exe.bat first...
    call build_exe.bat
    if not exist "dist\CleanScreenGuard\CleanScreenGuard.exe" exit /b 1
)

set "ISCC="
if exist "%ProgramFiles%\Inno Setup 7\ISCC.exe" set "ISCC=%ProgramFiles%\Inno Setup 7\ISCC.exe"
if exist "%ProgramFiles(x86)%\Inno Setup 7\ISCC.exe" set "ISCC=%ProgramFiles(x86)%\Inno Setup 7\ISCC.exe"
if exist "%LOCALAPPDATA%\Programs\Inno Setup 7\ISCC.exe" set "ISCC=%LOCALAPPDATA%\Programs\Inno Setup 7\ISCC.exe"
if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" set "ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe"
if exist "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" set "ISCC=%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"

if "%ISCC%"=="" (
    echo [ERROR] Inno Setup was not found on your PC.
    echo.
    echo Install Inno Setup 7 ^(free, ~14 MB^) from:
    echo   https://jrsoftware.org/inno-setup/downloads
    echo Or run in Terminal:
    echo   winget install --id JRSoftware.InnoSetup.7 -e -s winget -i
    echo.
    echo Then double-click build_installer.bat again!
    pause
    exit /b 1
)

echo Found Inno Setup compiler: "%ISCC%"
echo Compiling CleanScreenGuard_Setup_1.0.0.exe...
"%ISCC%" installer.iss
if errorlevel 1 (
    echo.
    echo [ERROR] Installer compilation failed!
    pause
    exit /b 1
)

echo.
echo ====================================================
echo   INSTALLER READY!
echo ====================================================
echo Your Windows installer is ready at:
echo   dist\CleanScreenGuard_Setup_1.0.0.exe
echo.
explorer "dist"
pause
