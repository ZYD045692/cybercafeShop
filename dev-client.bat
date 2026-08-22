@echo off
rem dev: client (vite hot reload + tauri shell, connects to 127.0.0.1)
cd /d %~dp0
call npm run dev:client
pause
