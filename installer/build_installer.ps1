# 本地一键构建 GPUCap Windows x64 安装包脚本
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

# 从 Cargo.toml 解析当前版本号
$verLine = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
$version = if ($verLine) { $verLine.Matches[0].Groups[1].Value } else { "0.2.0" }
Write-Host "[GPUCap] Target version: $version" -ForegroundColor Cyan

if (-not $SkipBuild) {
    Write-Host "[GPUCap] Building Web UI..." -ForegroundColor Cyan
    npm --prefix .\web run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "[GPUCap] Building Rust Release binary..." -ForegroundColor Cyan
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not (Test-Path "target\release\gpucap.exe")) {
    Write-Error "Missing target\release\gpucap.exe. Run without -SkipBuild first."
}

# 查找 Inno Setup 6 编译器 ISCC.exe
$iscc = $null
$cmd = Get-Command iscc -ErrorAction SilentlyContinue
if ($cmd) {
    $iscc = $cmd.Source
} else {
    foreach ($p in @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $p) { $iscc = $p; break }
    }
}

if (-not $iscc) {
    Write-Error "Inno Setup 6 (ISCC.exe) not found. Install it via: winget install JRSoftware.InnoSetup"
}

New-Item -ItemType Directory -Force -Path "dist-win" | Out-Null
Copy-Item -Force "target\release\gpucap.exe" "dist-win\gpucap.exe"

Write-Host "[GPUCap] Compiling Windows installer with $iscc ..." -ForegroundColor Cyan
& $iscc "/DAppVersion=$version" "installer\gpucap.iss"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[GPUCap] Done! Outputs:" -ForegroundColor Green
Get-ChildItem "dist-win" | Select-Object Name, Length
