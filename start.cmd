@echo off
chcp 65001 >nul
cd /d "%~dp0"

where node >nul 2>nul
if errorlevel 1 (
  echo [!] 需要 Node.js 18+ 以及 Rust 工具链才能构建并启动本地预览。
  pause
  exit /b 1
)

echo Inkdown · 浏览器预览服务器
echo 服务地址 http://127.0.0.1:8899/
echo 关闭此窗口即可停止服务。

start "" "http://127.0.0.1:8899/"
node tools\dev.mjs
