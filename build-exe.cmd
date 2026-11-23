@echo off
chcp 65001 >nul
cd /d "%~dp0"
setlocal
set "RC=0"

rem ============================================================
rem  Inkdown · 重建桌面端 exe
rem
rem  用法：
rem    build-exe.cmd         增量重建（改过代码就跑这个）
rem    build-exe.cmd clean   先删掉 target\ 与 ui\dist\ 再全量重建
rem
rem  流程：编译 Leptos WASM 前端（tauri 的 beforeBuildCommand）
rem        → 编 Rust release → 打 NSIS 安装包
rem ============================================================

call :ensure_rc
if errorlevel 1 goto :done

where node >nul 2>nul
if errorlevel 1 goto :no_node

where cargo >nul 2>nul
if errorlevel 1 goto :no_cargo

call :check_not_running
if errorlevel 1 goto :done

if not exist "node_modules" (
  echo [准备] 安装前端依赖…
  call npm install
  if errorlevel 1 goto :build_failed
)

if /i "%~1"=="clean" goto :do_clean
goto :skip_clean

:do_clean
echo [清理] 删除 target\ 与 ui\dist\ …
if exist "target" rmdir /s /q "target"
if exist "ui\dist" rmdir /s /q "ui\dist"

:skip_clean
echo [构建] 前端 WASM + Rust release + NSIS 打包…
echo.
call npx tauri build
if errorlevel 1 goto :build_failed

echo.
echo ============================================================
echo  构建完成
echo ============================================================
if exist "target\release\inkdown.exe" echo   可执行文件  %CD%\target\release\inkdown.exe
if exist "target\release\bundle\nsis\*.exe" for %%f in ("target\release\bundle\nsis\*.exe") do echo   安装包      %%~ff
echo.
goto :done

:no_node
echo [!] 需要 Node.js 18+：https://nodejs.org/
set "RC=1"
goto :done

:no_cargo
echo [!] 需要 Rust 工具链：https://rustup.rs/
set "RC=1"
goto :done

:build_failed
echo.
echo [!] 构建失败，请往上翻看日志。
set "RC=1"
goto :done

:done
rem 双击运行时留住窗口看结果；自动化调用设 INKDOWN_BUILD_NO_PAUSE=1 跳过
if not "%INKDOWN_BUILD_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%

rem ------------------------------------------------------------
rem  release 打包时 tauri 要用 rc.exe 把图标和版本信息嵌进 exe。
rem  负责这一步的 embed-resource 只认 RC / RC_<TARGET> 环境变量或
rem  注册表里的 SDK 安装，不会去系统 PATH 里找——所以就算 where rc
rem  能找到，从资源管理器双击运行时打包仍会失败。这里自己定位一份
rem  x64 的 rc.exe 并导出 RC，绕开那条不可靠的探测链。
rem ------------------------------------------------------------
:ensure_rc
if defined RC exit /b 0
call :find_sdk_rc
if defined RC exit /b 0
echo [!] 找不到 RC.EXE（Windows 资源编译器），release 打包会失败。
echo     请安装 Visual Studio 的「使用 C++ 的桌面开发」或 Windows SDK 后重试。
set "RC=1"
exit /b 1

:find_sdk_rc
set "SDKROOT=%ProgramFiles(x86)%\Windows Kits\10\bin"
if not exist "%SDKROOT%" exit /b 0
rem 目录名按版本排序，从新往旧找第一个带 x64\rc.exe 的
for /f "delims=" %%d in ('dir /b /ad /o-n "%SDKROOT%" 2^>nul') do call :try_sdk "%%d"
exit /b 0

:try_sdk
if defined RC exit /b 0
if exist "%SDKROOT%\%~1\x64\rc.exe" set "RC=%SDKROOT%\%~1\x64\rc.exe"
exit /b 0

rem ------------------------------------------------------------
rem  跑着的 Inkdown 会锁住 target\release\inkdown.exe，
rem  cargo 换不掉它，只会抛一句没头没尾的「拒绝访问」。
rem  与其让它编到一半才失败，不如开头就说清楚。
rem ------------------------------------------------------------
:check_not_running
tasklist /fi "imagename eq inkdown.exe" 2>nul | find /i "inkdown.exe" >nul
if errorlevel 1 exit /b 0
echo [!] Inkdown 正在运行，exe 被它占用，重新构建会失败。
echo     请先关掉 Inkdown 窗口再执行本脚本。
set "RC=1"
exit /b 1
