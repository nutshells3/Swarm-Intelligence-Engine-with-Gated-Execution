@echo off
title SIEGE Demo Launcher
cd /d "%~dp0"

echo.
echo   SIEGE - Swarm Intelligence Engine with Gated Execution
echo.

taskkill /F /IM orchestration-api.exe 2>nul
taskkill /F /IM loop-runner.exe 2>nul
taskkill /F /IM worker-dispatch.exe 2>nul

echo [1] PostgreSQL...
docker compose up -d postgres

echo [2] API server (compiling...)
start "SIEGE API" cmd /k "cd /d %~dp0 && set SIEGE_DEMO_MODE=1 && set ORCHESTRATION_DATABASE_URL=postgres://postgres:postgres@localhost/development_swarm && cargo run --manifest-path services\orchestration-api\Cargo.toml"

:wait_api
curl -s -o nul http://127.0.0.1:8845/health >nul 2>&1 && goto api_ok
ping 127.0.0.1 -n 4 >nul
goto wait_api

:api_ok
echo [3] Loop Runner...
start "SIEGE Loop" cmd /k "cd /d %~dp0 && set SIEGE_DEMO_MODE=1 && set ORCHESTRATION_DATABASE_URL=postgres://postgres:postgres@localhost/development_swarm && cargo run --manifest-path services\loop-runner\Cargo.toml"

echo [4] Worker Dispatch...
start "SIEGE Workers" cmd /k "cd /d %~dp0 && set SIEGE_DEMO_MODE=1 && set ORCHESTRATION_DATABASE_URL=postgres://postgres:postgres@localhost/development_swarm && cargo run --manifest-path services\worker-dispatch\Cargo.toml"

echo [5] Web UI...
start "SIEGE Web" cmd /k "cd /d %~dp0apps\web && npm run dev"

start http://localhost:5173

echo.
echo   API:     http://127.0.0.1:8845
echo   Swagger: http://127.0.0.1:8845/swagger-ui/
echo   Web:     http://localhost:5173
echo.
pause
