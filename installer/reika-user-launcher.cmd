@echo off
setlocal
rem Every action in this file is intended to run as the original interactive user.
set "REIKA_INSTALL_MODE=installed"
set "REIKA_ORIGINAL_USER_LOCALAPPDATA=%LOCALAPPDATA%"
if /I "%~1"=="register" goto register
if /I "%~1"=="watchdog" goto watchdog
start "REIKA Printer Service" /d "%~dp0" "%~dp0reika-escpos.exe"
exit /b 0
:register
schtasks /Create /F /SC ONLOGON /RL LIMITED /TN "REIKA\Watchdog" /TR "\"%~dp0reika-user-launcher.cmd\" watchdog" >nul
exit /b %ERRORLEVEL%
:watchdog
cscript.exe //nologo "%~dp0reika-watchdog.vbs"
exit /b %ERRORLEVEL%
