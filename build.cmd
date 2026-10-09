@echo off
rem Build PicaLum QwenImage from CMD, or double-click this file in Explorer.
rem Usage: .\build.cmd            app + CLI into .\out
rem        .\build.cmd -Installer also the NSIS installer
setlocal
set "PS=powershell"
where pwsh >/dev/null 2>/dev/null && set "PS=pwsh"
%PS% -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*
set "CODE=%ERRORLEVEL%"
rem Explorer starts batch files as: cmd /c ""C:\path\build.cmd" ". Only then keep the window open.
set "CL=%CMDCMDLINE:"=?%"
if /i not "%CL:.cmd? ?=%"=="%CL%" pause
exit /b %CODE%
