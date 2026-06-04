<#
.SYNOPSIS
    构建 Android APK（含签名）。
.DESCRIPTION
    调用 scripts\build_android_apk.ps1 完成编译后，使用 release keystore 重新签名。
    若 keystore 不存在则自动通过 keytool 生成。
.PARAMETER Target
    目标 ABI，可选 aarch64 / armv7 / i686 / x86_64，默认 aarch64。
.PARAMETER KeystorePath
    Release keystore 路径，默认 src-tauri\release.keystore。
.PARAMETER KeystoreAlias
    Keystore 别名，默认 dream-narrative。
.PARAMETER KeystorePassword
    Keystore 密码（不传则交互式输入）。
.PARAMETER Mode
    release：使用 release keystore 签名；debug：仅用 debug.keystore（默认）。
.EXAMPLE
    .\build-apk.ps1 -Mode release -KeystorePassword "mypassword"
#>
param(
    [ValidateSet("aarch64", "armv7", "i686", "x86_64")]
    [string]$Target = "aarch64",
    [string]$KeystorePath = "",
    [string]$KeystoreAlias = "dream-narrative",
    [string]$KeystorePassword = "",
    [ValidateSet("debug", "release")]
    [string]$Mode = "release"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path $PSScriptRoot).Path

Set-Location $RepoRoot

$ArtifactDir = Join-Path $RepoRoot "artifacts\android"
New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null

# --- Keystore setup ---
$JAVA_HOME = if ($env:JAVA_HOME) { $env:JAVA_HOME } else { "C:\Program Files\Eclipse Adoptium\jdk-17.0.18.8-hotspot" }
$Keytool = Join-Path $JAVA_HOME "bin\keytool.exe"

if (-not $KeystorePath) {
    $KeystorePath = Join-Path $RepoRoot "src-tauri\release.keystore"
}

if ($Mode -eq "release") {
    if (-not (Test-Path $Keytool)) {
        throw "keytool not found at: $Keytool"
    }

    if (-not (Test-Path $KeystorePath)) {
        if (-not $KeystorePassword) {
            $KeystorePassword = Read-Host -Prompt "Enter keystore password for new keystore"
        }

        $dname = "CN=Dream Narrative Engine, OU=Dev, O=DreamNarrative, L=Unknown, ST=Unknown, C=CN"
        Write-Host "Generating release keystore: $KeystorePath" -ForegroundColor Cyan
        & $Keytool -genkey -v `
            -keystore $KeystorePath `
            -alias $KeystoreAlias `
            -keyalg RSA `
            -keysize 2048 `
            -validity 10000 `
            -storepass $KeystorePassword `
            -keypass $KeystorePassword `
            -dname $dname

        if ($LASTEXITCODE -ne 0) {
            throw "Keystore generation failed."
        }
    }
}

# --- Build ---
Write-Host "Building APK (target: $Target)..." -ForegroundColor Cyan

$BuildScript = Join-Path $RepoRoot "scripts\build_android_apk.ps1"
if (-not (Test-Path $BuildScript)) {
    throw "Build script not found: $BuildScript"
}

& $BuildScript -Target $Target
if ($LASTEXITCODE -ne 0) {
    throw "Android APK build failed."
}

# --- Release signing ---
if ($Mode -eq "release") {
    $ApkOutputDir = Join-Path $RepoRoot "src-tauri\gen\android\app\build\outputs\apk"

    $unsignedApks = Get-ChildItem -Path $ApkOutputDir -Recurse -Filter "*universal-release-unsigned.apk" -File |
        Sort-Object LastWriteTime -Descending

    if (-not $unsignedApks -or $unsignedApks.Count -eq 0) {
        $unsignedApks = Get-ChildItem -Path $ApkOutputDir -Recurse -Filter "*-unsigned.apk" -File |
            Sort-Object LastWriteTime -Descending
    }

    if (-not $unsignedApks -or $unsignedApks.Count -eq 0) {
        Write-Host "No unsigned APK found — using already-signed or debug-signed APK." -ForegroundColor Yellow
        $signedApks = Get-ChildItem -Path $ApkOutputDir -Recurse -Filter "*-debugsigned.apk" -File |
            Sort-Object LastWriteTime -Descending
        if ($signedApks) {
            $latestApk = $signedApks | Select-Object -First 1
        }
    } else {
        $unsignedApk = $unsignedApks | Select-Object -First 1

        if (-not $KeystorePassword) {
            $KeystorePassword = Read-Host -Prompt "Enter keystore password"
        }

        if (-not (Test-Path $KeystorePath)) {
            throw "Keystore not found: $KeystorePath"
        }

        $BuildToolsDir = Get-ChildItem -Path (Join-Path $env:ANDROID_SDK_ROOT "build-tools") -Directory |
            Sort-Object Name -Descending |
            Select-Object -First 1
        $Apksigner = Join-Path $BuildToolsDir.FullName "apksigner.bat"

        $signedApk = Join-Path $unsignedApk.DirectoryName ($unsignedApk.BaseName -replace "-unsigned$", "-release-signed")
        $signedApk = "$signedApk$($unsignedApk.Extension)"

        if (Test-Path $signedApk) {
            Remove-Item -LiteralPath $signedApk -Force
        }

        Write-Host "Signing APK with release keystore..." -ForegroundColor Cyan
        & $Apksigner sign `
            --ks $KeystorePath `
            --ks-key-alias $KeystoreAlias `
            --ks-pass "pass:$KeystorePassword" `
            --key-pass "pass:$KeystorePassword" `
            --out $signedApk `
            $unsignedApk.FullName

        if ($LASTEXITCODE -ne 0) {
            throw "APK signing failed."
        }

        $latestApk = Get-Item $signedApk
    }
} else {
    $ApkOutputDir = Join-Path $RepoRoot "src-tauri\gen\android\app\build\outputs\apk"
    $latestApk = Get-ChildItem -Path $ApkOutputDir -Recurse -Filter "*.apk" -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

if (-not $latestApk) {
    throw "No APK artifact found."
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$copiedApk = Join-Path $ArtifactDir ("{0}-{1}{2}" -f $latestApk.BaseName, $stamp, $latestApk.Extension)
Copy-Item -Path $latestApk.FullName -Destination $copiedApk -Force

Write-Host ""
Write-Host "=== APK Build Complete ===" -ForegroundColor Green
Write-Host "Artifact: $copiedApk" -ForegroundColor Green
Write-Host ""