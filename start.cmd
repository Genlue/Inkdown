@echo off
chcp 65001 >nul
cd /d "%~dp0"

where python >nul 2>nul
if errorlevel 1 (
  echo [!] 需要 Python 3 才能启动本地静态服务。
  echo     安装后重新运行本脚本，或用任意静态服务器打开本目录。
  pause
  exit /b 1
)

echo Inkdown · Web Demo
echo 服务地址 http://127.0.0.1:8899/index.html
echo 关闭这个窗口即可停止服务。

start "Inkdown Server" /min cmd /c "python -m http.server 8899 --directory web"
timeout /t 2 >nul
start "" "http://127.0.0.1:8899/index.html"
