@echo off
rem Measures the frame rate from five fixed views and what each part of the
rem world costs. Takes about a minute and a half. The result is shown in the
rem window and written to measure.txt.
cd /d %~dp0
git pull
set GRAV_MEASURE=1
rem Without this the engine waits for the screen's refresh and every number reads 60.
set ENGINE_PACE=0
cargo run --release -p grav
echo.
type measure.txt
pause
