param(
    [ValidateSet("aarch64", "armv7", "i686", "x86_64")]
    [string]$Target = "aarch64",
    [string]$IconSource = "",
    [switch]$SkipFrontendInstall,
    [switch]$SkipRootInstall
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$artifactDir = Join-Path $repoRoot "artifacts\android"
$apkOutputDir = Join-Path $repoRoot "src-tauri\gen\android\app\build\outputs\apk"
$gradleUserHomeRoot = Join-Path $repoRoot ".gradle-android"
$androidProjectDir = Join-Path $repoRoot "src-tauri\gen\android"
$androidResDir = Join-Path $androidProjectDir "app\src\main\res"
$androidIconSourceDir = Join-Path $repoRoot "src-tauri\icons\android"
$iconGenerator = Join-Path $repoRoot "scripts\generate_app_icons.py"
$defaultIconSource = Join-Path $repoRoot "src-tauri\icons\source-launcher.png"
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$androidStringsPath = Join-Path $androidResDir "values\strings.xml"
$gradleWrapper = Join-Path $androidProjectDir "gradlew.bat"
$sdkManager = Join-Path $env:ANDROID_SDK_ROOT "cmdline-tools\latest\bin\sdkmanager.bat"
$android36Dir = Join-Path $env:ANDROID_SDK_ROOT "platforms\android-36"
$android36Jar = Join-Path $android36Dir "android.jar"
$cargoAndroidLock = Join-Path $repoRoot "src-tauri\target\aarch64-linux-android\release\lock.android"
$debugKeystore = Join-Path $env:USERPROFILE ".android\debug.keystore"
$defaultGradleUserHome = Join-Path $env:USERPROFILE ".gradle"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"

function Remove-FileWithRetry {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LiteralPath,
        [int]$Retries = 8,
        [int]$DelayMilliseconds = 750
    )

    for ($attempt = 1; $attempt -le $Retries; $attempt++) {
        try {
            Remove-Item -LiteralPath $LiteralPath -Force
            return
        } catch {
            if ($attempt -ge $Retries) {
                throw "Unable to remove stale file because it is still in use: $LiteralPath. Close Explorer preview/properties windows, stop any adb install using this APK, or wait for antivirus scanning to finish, then retry. Original error: $($_.Exception.Message)"
            }

            Start-Sleep -Milliseconds $DelayMilliseconds
        }
    }
}

function Ensure-AndroidAdjustResize {
    param([string]$ManifestPath)

    if (-not (Test-Path $ManifestPath)) {
        return
    }

    $content = [System.IO.File]::ReadAllText($ManifestPath, [System.Text.Encoding]::UTF8)
    if ($content -match 'android:windowSoftInputMode=') {
        return
    }

    $updated = $content -replace '(android:configChanges="[^"]*"\s*)', "`$1            android:windowSoftInputMode=`"adjustResize`"`r`n"
    if ($updated -ne $content) {
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($ManifestPath, $updated, $utf8NoBom)
    }
}

if (-not $env:JAVA_HOME) {
    $env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.18.8-hotspot"
}
if (-not $env:ANDROID_HOME) {
    $env:ANDROID_HOME = "C:\Users\HS\android-sdk"
}
if (-not $env:ANDROID_SDK_ROOT) {
    $env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
}

if (-not (Test-Path $env:JAVA_HOME)) {
    throw "JAVA_HOME not found: $env:JAVA_HOME"
}
if (-not (Test-Path $env:ANDROID_SDK_ROOT)) {
    throw "ANDROID_SDK_ROOT not found: $env:ANDROID_SDK_ROOT"
}
if (-not (Test-Path $sdkManager)) {
    throw "sdkmanager not found: $sdkManager"
}
if (-not (Test-Path $gradleWrapper)) {
    throw "Gradle wrapper not found: $gradleWrapper"
}

Push-Location $repoRoot
try {
    $buildStartedAt = Get-Date
    $runStamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $freshArtifactThreshold = $buildStartedAt.AddSeconds(-2)
    $gradleUserHome = Join-Path $gradleUserHomeRoot $runStamp
    Write-Host "Checking dependencies..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Force -Path $gradleUserHomeRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $gradleUserHome | Out-Null

    foreach ($sharedDirName in @("caches", "wrapper")) {
        $sourceDir = Join-Path $defaultGradleUserHome $sharedDirName
        $targetDir = Join-Path $gradleUserHome $sharedDirName

        if ((Test-Path $sourceDir) -and -not (Test-Path $targetDir)) {
            New-Item -ItemType Junction -Path $targetDir -Target $sourceDir | Out-Null
        }
    }

    # Use an isolated Gradle home so user-level proxy settings do not break SDK downloads.
    $env:GRADLE_USER_HOME = $gradleUserHome
    foreach ($proxyVar in @("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY", "http_proxy", "https_proxy", "all_proxy", "no_proxy")) {
        if (Test-Path "Env:$proxyVar") {
            Remove-Item "Env:$proxyVar"
        }
    }

    Write-Host "Stopping previous Android build processes..." -ForegroundColor Cyan
    & $gradleWrapper --project-dir $androidProjectDir --stop | Out-Host
    Start-Sleep -Seconds 2

    $staleJava = Get-CimInstance Win32_Process |
        Where-Object {
            $_.Name -eq "java.exe" -and
            $_.CommandLine -and
            (
                $_.CommandLine -like "*$androidProjectDir*" -or
                $_.CommandLine -like "*GradleWrapperMain*"
            )
        }

    foreach ($proc in $staleJava) {
        try {
            Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
        }
        catch {
            Write-Host "Skip stopping PID $($proc.ProcessId): $($_.Exception.Message)" -ForegroundColor DarkYellow
        }
    }

    $staleCmd = Get-CimInstance Win32_Process |
        Where-Object {
            $_.Name -eq "cmd.exe" -and
            $_.CommandLine -and
            $_.CommandLine -like "*$androidProjectDir*"
        }

    foreach ($proc in $staleCmd) {
        try {
            Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
        }
        catch {
            Write-Host "Skip stopping PID $($proc.ProcessId): $($_.Exception.Message)" -ForegroundColor DarkYellow
        }
    }

    Start-Sleep -Seconds 2

    if (Test-Path $cargoAndroidLock) {
        try {
            Remove-Item -LiteralPath $cargoAndroidLock -Force -ErrorAction Stop
        }
        catch {
            Write-Host "Cargo Android lock is still in use, build will wait for release." -ForegroundColor DarkYellow
        }
    }

    if (-not $SkipRootInstall -and -not (Test-Path "node_modules")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            throw "Root npm install failed."
        }
    }

    if (-not $SkipFrontendInstall -and -not (Test-Path "frontend\node_modules")) {
        npm --prefix frontend install
        if ($LASTEXITCODE -ne 0) {
            throw "Frontend npm install failed."
        }
    }

    New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

    if (Test-Path $iconGenerator) {
        $resolvedIconSource = if ($IconSource) { (Resolve-Path $IconSource).Path } else { $defaultIconSource }
        if (Test-Path $resolvedIconSource) {
            Write-Host "Generating app icons from $resolvedIconSource ..." -ForegroundColor Cyan
            & python $iconGenerator $resolvedIconSource
            if ($LASTEXITCODE -ne 0) {
                throw "Icon generation failed."
            }
        }
    }

    if (Test-Path $androidIconSourceDir) {
        Write-Host "Syncing Android launcher icons..." -ForegroundColor Cyan
        $androidIconDirs = @(
            "mipmap-anydpi-v26",
            "mipmap-hdpi",
            "mipmap-mdpi",
            "mipmap-xhdpi",
            "mipmap-xxhdpi",
            "mipmap-xxxhdpi",
            "values"
        )

        foreach ($dirName in $androidIconDirs) {
            $sourceDir = Join-Path $androidIconSourceDir $dirName
            $targetDir = Join-Path $androidResDir $dirName

            if (-not (Test-Path $sourceDir)) {
                continue
            }

            New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
            Copy-Item -Path (Join-Path $sourceDir "*") -Destination $targetDir -Recurse -Force
        }
    }

    if ((Test-Path $tauriConfigPath) -and (Test-Path $androidStringsPath)) {
        $productName = (Get-Content -Path $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json).productName
        if ($productName) {
            Write-Host "Syncing Android app name..." -ForegroundColor Cyan
            # Use Python to write without BOM
            $fixScript = Join-Path $repoRoot "scripts\fix_strings.py"
            if (Test-Path $fixScript) {
                & python $fixScript
            } else {
                # Fallback: write directly with UTF8NoBOM encoding
                $xmlContent = @"
<resources>
    <string name="app_name">$productName</string>
    <string name="main_activity_title">$productName</string>
</resources>
"@
                $utf8NoBom = New-Object System.Text.UTF8Encoding $false
                [System.IO.File]::WriteAllText($androidStringsPath, $xmlContent, $utf8NoBom)
            }
        }
    }

    Ensure-AndroidAdjustResize -ManifestPath (Join-Path $androidProjectDir "app\src\main\AndroidManifest.xml")

    if (-not (Test-Path $android36Jar)) {
        Write-Host "Repairing Android SDK Platform 36..." -ForegroundColor Cyan
        if (Test-Path $android36Dir) {
            Remove-Item -LiteralPath $android36Dir -Recurse -Force
        }
        @("y", "y", "y", "y", "y") | & $sdkManager "--sdk_root=$env:ANDROID_SDK_ROOT" "platforms;android-36"
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $android36Jar)) {
            throw "Android SDK Platform 36 install failed."
        }
    }

    if (Test-Path $apkOutputDir) {
        Write-Host "Removing stale APK outputs..." -ForegroundColor Cyan
        Get-ChildItem -Path $apkOutputDir -Recurse -Filter *.apk -File | ForEach-Object {
            Remove-FileWithRetry -LiteralPath $_.FullName
        }
    }

    $abiList = "arm64-v8a"
    $archList = "arm64"
    switch ($Target) {
        "armv7" {
            $abiList = "armeabi-v7a"
            $archList = "arm"
        }
        "i686" {
            $abiList = "x86"
            $archList = "x86"
        }
        "x86_64" {
            $abiList = "x86_64"
            $archList = "x86_64"
        }
    }

    Write-Host "Building Android APK..." -ForegroundColor Cyan
    npx tauri android build --apk --ci -t $Target
    $tauriExitCode = $LASTEXITCODE

    if ($tauriExitCode -ne 0) {
        Write-Host "Tauri build failed, retrying Android packaging from local Gradle cache..." -ForegroundColor DarkYellow
        $env:GRADLE_USER_HOME = $defaultGradleUserHome
        & $gradleWrapper `
            --project-dir $androidProjectDir `
            --no-daemon `
            --offline `
            assembleUniversalRelease `
            "-PabiList=$abiList" `
            "-ParchList=$archList" `
            "-PtargetList=$Target"
        $tauriExitCode = $LASTEXITCODE
    }

    $generatedApks = Get-ChildItem -Path $apkOutputDir -Recurse -Filter *.apk -File |
        Sort-Object LastWriteTime -Descending
    $freshGeneratedApks = $generatedApks |
        Where-Object { $_.LastWriteTime -ge $freshArtifactThreshold }

    if (-not $freshGeneratedApks -or $freshGeneratedApks.Count -eq 0) {
        if (-not $generatedApks -or $generatedApks.Count -eq 0) {
            throw "Build completed, but no APK output was found."
        }
        throw "Build completed, but no fresh APK was generated for this run. Refusing to archive stale artifacts."
    }

    $latestApk = $freshGeneratedApks | Select-Object -First 1
    if ($tauriExitCode -ne 0) {
        Write-Host "Tauri reported a non-zero exit code, checking generated APKs..." -ForegroundColor DarkYellow
    }

    $unsignedApk = $freshGeneratedApks |
        Where-Object {
            $_.Name -like "*-unsigned.apk"
        } |
        Select-Object -First 1

    if ($tauriExitCode -ne 0 -and -not $unsignedApk) {
        throw "Tauri Android APK build failed."
    }

    if ($unsignedApk) {
        $buildToolsDir = Get-ChildItem -Path (Join-Path $env:ANDROID_SDK_ROOT "build-tools") -Directory |
            Sort-Object Name -Descending |
            Select-Object -First 1

        if (-not $buildToolsDir) {
            throw "Android build-tools not found."
        }

        $apksigner = Join-Path $buildToolsDir.FullName "apksigner.bat"
        if (-not (Test-Path $apksigner)) {
            throw "apksigner not found: $apksigner"
        }
        if (-not (Test-Path $debugKeystore)) {
            throw "Debug keystore not found: $debugKeystore"
        }

        $signedApk = Join-Path $unsignedApk.DirectoryName ($unsignedApk.BaseName -replace "-unsigned$", "-debugsigned")
        $signedApk = "$signedApk$($unsignedApk.Extension)"

        if (Test-Path $signedApk) {
            Remove-Item -LiteralPath $signedApk -Force
        }

        Write-Host "Signing APK with debug keystore..." -ForegroundColor Cyan
        & $apksigner sign `
            --ks $debugKeystore `
            --ks-key-alias androiddebugkey `
            --ks-pass pass:android `
            --key-pass pass:android `
            --out $signedApk `
            $unsignedApk.FullName

        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $signedApk)) {
            throw "APK signing failed."
        }

        $latestApk = Get-Item $signedApk
    }

    $copiedApk = Join-Path $artifactDir ("{0}-{1}{2}" -f $latestApk.BaseName, $stamp, $latestApk.Extension)
    Copy-Item -Path $latestApk.FullName -Destination $copiedApk -Force

    Write-Host ""
    Write-Host "Build completed." -ForegroundColor Green
    Write-Host "Source APK: $($latestApk.FullName)" -ForegroundColor Green
    Write-Host "Archived APK: $copiedApk" -ForegroundColor Green
}
finally {
    Pop-Location
}
