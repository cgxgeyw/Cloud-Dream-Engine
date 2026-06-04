param(
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

Set-Location $RepoRoot

Write-Host "Checking frontend dependencies..." -ForegroundColor Cyan
if (-not (Test-Path "frontend\node_modules")) {
    npm --prefix frontend install
}
if (-not (Test-Path "node_modules")) {
    npm install
}

Write-Host "Building backend sidecar..." -ForegroundColor Cyan
powershell -ExecutionPolicy Bypass -File .\scripts\build-backend-sidecar.ps1 -RepoRoot $RepoRoot
if ($LASTEXITCODE -ne 0) {
    throw "Backend sidecar build failed."
}

Write-Host "Building frontend..." -ForegroundColor Cyan
npm --prefix frontend run build

Write-Host "Building Windows MSI with Tauri..." -ForegroundColor Cyan
npm run tauri -- build --target $Target
if ($LASTEXITCODE -ne 0) {
    throw "Tauri MSI build failed."
}

Write-Host "Windows MSI build finished." -ForegroundColor Green
