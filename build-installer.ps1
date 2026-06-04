<#
.SYNOPSIS
    构建 Windows 安装程序（.msi）。
.DESCRIPTION
    构建前端 → 编译后端 sidecar → Tauri Windows 打包 → 输出 .msi 产物路径。
.PARAMETER Target
    Rust target triple，默认 x86_64-pc-windows-msvc。
.PARAMETER SkipSidecar
    跳过后端 sidecar 编译。
.EXAMPLE
    .\build-installer.ps1
.EXAMPLE
    .\build-installer.ps1 -SkipSidecar
#>
param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$SkipSidecar
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path $PSScriptRoot).Path

Set-Location $RepoRoot

$ArtifactDir = Join-Path $RepoRoot "artifacts\windows"
New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null

# --- Frontend ---
Write-Host "[1/3] Preparing frontend..." -ForegroundColor Cyan
if (-not (Test-Path "frontend\node_modules")) {
    npm --prefix frontend install
    if ($LASTEXITCODE -ne 0) { throw "Frontend npm install failed." }
}
if (-not (Test-Path "node_modules")) {
    npm install
    if ($LASTEXITCODE -ne 0) { throw "Root npm install failed." }
}

npm --prefix frontend run build
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed." }

# --- Backend Sidecar ---
if (-not $SkipSidecar) {
    Write-Host "[2/3] Building backend sidecar..." -ForegroundColor Cyan
    powershell -ExecutionPolicy Bypass -File "$RepoRoot\scripts\build-backend-sidecar.ps1" -RepoRoot $RepoRoot
    if ($LASTEXITCODE -ne 0) { throw "Backend sidecar build failed." }
}

# --- Tauri Windows Build ---
Write-Host "[3/3] Building Windows installer (Tauri)..." -ForegroundColor Cyan
npm run tauri -- build --target $Target
if ($LASTEXITCODE -ne 0) { throw "Tauri Windows build failed." }

# --- Collect artifacts ---
$TauriTargetDir = Join-Path $RepoRoot "src-tauri\target\$Target\release\bundle"

$msiFiles = Get-ChildItem -Path $TauriTargetDir -Recurse -Filter "*.msi" -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending

$exeFiles = Get-ChildItem -Path $TauriTargetDir -Recurse -Filter "*.exe" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "*setup*" -or $_.Name -like "*install*" } |
    Sort-Object LastWriteTime -Descending

if (-not $msiFiles -and -not $exeFiles) {
    Write-Host "No .msi or setup .exe found under $TauriTargetDir — check Tauri bundle output." -ForegroundColor Yellow
} else {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    foreach ($f in @($msiFiles) + @($exeFiles)) {
        $dest = Join-Path $ArtifactDir ("{0}-{1}{2}" -f $f.BaseName, $stamp, $f.Extension)
        Copy-Item -Path $f.FullName -Destination $dest -Force
        Write-Host "  Copied: $dest" -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "=== Windows Installer Build Complete ===" -ForegroundColor Green
Write-Host "Artifact directory: $ArtifactDir" -ForegroundColor Green
Write-Host ""