@echo off
rem dev: admin (vite hot reload + tauri shell, data in dev-data\)
cd /d %~dp0
call npm run dev:admin
pause
