@echo off
rem one-click build: compile both apps and assemble dist\
cd /d %~dp0
call npm run pack
pause
