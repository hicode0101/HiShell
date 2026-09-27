@echo off
setlocal
rem ============================================================
rem  HiShell 浏览器快速预览（mock 模式）
rem
rem  双击运行：启动 Vite 开发服务器并自动打开浏览器。
rem  - 不编译 Rust，秒级启动，前端改动即时热更新（HMR）
rem  - 无真实 SSH 后端：会话/终端/SFTP 均为内置模拟数据，
rem    适合界面、交互、布局的预览与测试
rem  - 需要真实连接测试时请用:  build.bat dev
rem ============================================================

cd /d "%~dp0"

where node >nul 2>nul
if errorlevel 1 (
    echo [ERROR] 未找到 node，请先安装 Node.js 18+：https://nodejs.org
    pause
    exit /b 1
)

if not exist node_modules (
    echo [deps] 首次运行，安装前端依赖...
    call npm install --no-audit --no-fund
    if errorlevel 1 (
        echo [ERROR] npm install 失败
        pause
        exit /b 1
    )
)

rem 端口已被占用 = 预览服务已在运行（dev server 始终读取磁盘最新源码），直接打开浏览器
netstat -ano | findstr /c:":5173 " | findstr /i "LISTENING" >nul 2>nul
if not errorlevel 1 (
    echo [OK] 预览服务已在运行，直接打开浏览器：http://localhost:5173
    start "" http://localhost:5173
    exit /b 0
)

echo ==============================================
echo  HiShell 浏览器预览启动中（mock，无需编译 Rust）
echo  地址: http://localhost:5173   按 Ctrl+C 停止
echo ==============================================
call npx vite --open
pause
