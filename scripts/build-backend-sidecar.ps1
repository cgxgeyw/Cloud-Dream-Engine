param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

$ErrorActionPreference = "Stop"

$BackendBinaryName = "backend-desktop-x86_64-pc-windows-msvc"
$BinariesDir = Join-Path $RepoRoot "src-tauri\binaries"
$BuildDir = Join-Path $RepoRoot "build\pyinstaller"
$LegacyRoot = Join-Path $RepoRoot "legacy"
$BackendScript = Join-Path $LegacyRoot "backend\desktop_server.py"

Set-Location $RepoRoot

if (-not (Test-Path $BinariesDir)) {
    New-Item -ItemType Directory -Path $BinariesDir | Out-Null
}
if (-not (Test-Path $BuildDir)) {
    New-Item -ItemType Directory -Path $BuildDir | Out-Null
}
if (-not (Test-Path $BackendScript)) {
    throw "Backend entry script was not found: $BackendScript"
}

$OutputPath = Join-Path $BinariesDir "$BackendBinaryName.exe"
if (Test-Path $OutputPath) {
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            Remove-Item -LiteralPath $OutputPath -Force -ErrorAction Stop
            break
        }
        catch {
            if ($attempt -eq 5) {
                throw "Failed to remove existing backend sidecar: $OutputPath"
            }
            Start-Sleep -Seconds 1
        }
    }
}

python -m PyInstaller --version | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "PyInstaller was not found. Installing it for the active Python..." -ForegroundColor Yellow
    python -m pip install pyinstaller
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to install PyInstaller."
    }
}

python -m PyInstaller `
    $BackendScript `
    --name $BackendBinaryName `
    --onefile `
    --clean `
    --noconfirm `
    --distpath $BinariesDir `
    --workpath $BuildDir `
    --specpath $BuildDir `
    --paths $LegacyRoot `
    --collect-submodules backend `
    --collect-submodules fastapi `
    --collect-submodules uvicorn `
    --collect-submodules pydantic `
    --hidden-import multipart `
    --hidden-import python_multipart
if ($LASTEXITCODE -ne 0) {
    throw "PyInstaller failed."
}

if (-not (Test-Path $OutputPath)) {
    throw "Backend sidecar was not created: $OutputPath"
}

Write-Host "Backend sidecar ready: $OutputPath" -ForegroundColor Green
