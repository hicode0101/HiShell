@echo off
setlocal
rem ============================================================
rem  HiShell local Windows build script
rem  Usage:
rem    build.bat           release bundle (optimized, slow first time)
rem    build.bat debug     debug bundle (fast compile, for testing)
rem    build.bat exe       debug exe only, skip installer bundling (fastest)
rem    build.bat check     typecheck + cargo check only
rem  Artifacts:
rem    release: src-tauri\target\release\bundle\nsis\*.exe / msi\*.msi
rem    debug:   src-tauri\target\debug\bundle\nsis\*.exe
rem ============================================================

cd /d "%~dp0"
set MODE=%~1
if "%MODE%"=="" set MODE=release

echo ==============================================
echo  HiShell Windows build  [mode: %MODE%]
echo ==============================================

where node >nul 2>nul
if errorlevel 1 (
    echo [ERROR] node not found. Install Node.js 18+ from https://nodejs.org
    exit /b 1
)
where npm >nul 2>nul
if errorlevel 1 (
    echo [ERROR] npm not found. Reinstall Node.js.
    exit /b 1
)
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs
    echo         MSVC Build Tools are also required on Windows.
    exit /b 1
)

echo [env] node:
node --version
echo [env] npm:
call npm --version
echo [env] cargo:
cargo --version

if not exist node_modules (
    echo [deps] node_modules missing, running npm install...
    call npm install --no-audit --no-fund
    if errorlevel 1 (
        echo [ERROR] npm install failed
        exit /b 1
    )
)

if /i "%MODE%"=="check" goto :check
if /i "%MODE%"=="exe" goto :build_exe
if /i "%MODE%"=="debug" goto :build_debug
if /i "%MODE%"=="release" goto :build_release

echo [ERROR] unknown mode "%MODE%", expected: release / debug / exe / check
exit /b 1

:build_exe
echo [build] tauri build --debug --no-bundle ^(exe only, fastest^)
call npm run tauri build -- --debug --no-bundle
if errorlevel 1 (
    echo [ERROR] tauri build failed
    exit /b 1
)
echo.
echo ==============================================
echo  Debug exe OK!
echo ==============================================
echo Portable exe:
echo   %cd%\src-tauri\target\debug\hishell.exe
goto :eof

:build_release
echo [build] npm run build
call npm run build
if errorlevel 1 (
    echo [ERROR] frontend build failed
    exit /b 1
)
echo [build] tauri build ^(release, bundles NSIS/MSI, first run is slow^)
call npm run tauri build
if errorlevel 1 (
    echo [ERROR] tauri build failed
    exit /b 1
)
echo.
echo ==============================================
echo  Build OK! Installers:
echo ==============================================
dir /b src-tauri\target\release\bundle\nsis\*.exe 2>nul
dir /b src-tauri\target\release\bundle\msi\*.msi 2>nul
echo.
echo Paths:
echo   %cd%\src-tauri\target\release\bundle\nsis\
echo   %cd%\src-tauri\target\release\bundle\msi\
echo Portable exe:
echo   %cd%\src-tauri\target\release\hishell.exe
goto :eof

:build_debug
echo [build] tauri build --debug ^(fast compile, for local testing^)
call npm run tauri build -- --debug
if errorlevel 1 (
    echo [ERROR] tauri build --debug failed
    exit /b 1
)
echo.
echo ==============================================
echo  Debug build OK!
echo ==============================================
dir /b src-tauri\target\debug\bundle\nsis\*.exe 2>nul
echo.
echo Portable exe (recommended for testing):
echo   %cd%\src-tauri\target\debug\hishell.exe
goto :eof

:check
echo [check] vue-tsc typecheck
call npm run typecheck
if errorlevel 1 (
    echo [ERROR] typecheck failed
    exit /b 1
)
echo [check] cargo check
pushd src-tauri
cargo check
popd
if errorlevel 1 (
    echo [ERROR] cargo check failed
    exit /b 1
)
echo.
echo [OK] all checks passed
goto :eof
