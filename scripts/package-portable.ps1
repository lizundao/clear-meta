# Build release and package Windows portable ZIP.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1             # build + package
#   powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1 -SkipBuild  # package only
param(
    [string]$Version = "0.1.0",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

# 版本号会拼进输出路径并用于 Remove-Item，必须严格校验，防误删目录。
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([A-Za-z0-9.-]+)?$') {
    throw "Invalid version '$Version'. Expected e.g. 0.1.0"
}

$BuildStamp = Get-Date
if (-not $SkipBuild) {
    Write-Host "==> Building frontend and Tauri release..." -ForegroundColor Cyan
    & npm run tauri build
    if ($LASTEXITCODE -ne 0) {
        throw "tauri build failed with exit code $LASTEXITCODE. Packaging aborted."
    }
}

$ReleaseDir = Join-Path $Root "src-tauri\target\release"
$ExePath = Join-Path $ReleaseDir "clear-meta.exe"
if (-not (Test-Path $ExePath -PathType Leaf)) {
    throw "Release exe not found at $ExePath. Run without -SkipBuild first."
}

# 防止把上一次编译的旧 exe 打进新版本包。
$Exe = Get-Item $ExePath
if (-not $SkipBuild -and $Exe.LastWriteTime -lt $BuildStamp.AddSeconds(-5)) {
    throw "Release exe is older than this build ($($Exe.LastWriteTime)). Refusing to package a stale binary."
}
Write-Host "==> Found executable: $($Exe.FullName) ($($Exe.LastWriteTime))" -ForegroundColor Green

$PortableName = "ClearMeta-${Version}-portable-win-x64"
$ReleaseRoot = Join-Path $Root "release"
$OutDir = Join-Path $ReleaseRoot $PortableName
$ZipPath = Join-Path $ReleaseRoot "${PortableName}.zip"

if (-not (Test-Path $ReleaseRoot)) {
    New-Item -ItemType Directory -Path $ReleaseRoot -Force | Out-Null
}
# 只清理本次版本对应的输出目录，且必须位于 release/ 之内。
$ResolvedOut = [System.IO.Path]::GetFullPath($OutDir)
$ResolvedRoot = [System.IO.Path]::GetFullPath($ReleaseRoot)
if (-not $ResolvedOut.StartsWith($ResolvedRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe output path: $ResolvedOut"
}
if (Test-Path $ResolvedOut) { Remove-Item $ResolvedOut -Recurse -Force }
New-Item -ItemType Directory -Path $ResolvedOut -Force | Out-Null

Copy-Item $ExePath $ResolvedOut -Force
Get-ChildItem (Split-Path $ExePath -Parent) -File -ErrorAction SilentlyContinue | Where-Object {
    $_.Extension -in @(".dll", ".manifest", ".json") -and $_.Name -ne "clear-meta.exe"
} | ForEach-Object { Copy-Item $_.FullName $ResolvedOut -Force }

$DocsDir = Join-Path $ResolvedOut "docs"
New-Item -ItemType Directory -Path $DocsDir -Force | Out-Null
Get-ChildItem (Join-Path $Root "docs") -Filter "*.md" -File | ForEach-Object {
    Copy-Item $_.FullName $DocsDir -Force
}
foreach ($name in @("README.md", "LICENSE", "LICENSE.zh-CN.md")) {
    $src = Join-Path $Root $name
    if (Test-Path $src -PathType Leaf) { Copy-Item $src $ResolvedOut -Force }
}

# 中文文件名用字符码写出，避免 PowerShell 5.1 默认编码破坏文件名。
$notePath = Join-Path $ResolvedOut ([char]0x4f7f + [char]0x7528 + [char]0x8bf4 + [char]0x660e + ".txt")
$noteTemplate = Join-Path $Root "docs\portable-readme.txt"
if (Test-Path $noteTemplate -PathType Leaf) {
    $note = (Get-Content $noteTemplate -Raw -Encoding UTF8).Replace("{{VERSION}}", $Version)
    [System.IO.File]::WriteAllText($notePath, $note, [System.Text.UTF8Encoding]::new($true))
}

if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path $ResolvedOut -DestinationPath $ZipPath -Force

Write-Host "==> Portable folder: $ResolvedOut" -ForegroundColor Green
Write-Host "==> Portable ZIP:     $ZipPath" -ForegroundColor Green

$NsisDir = Join-Path $ReleaseDir "bundle\nsis"
if (Test-Path $NsisDir) {
    Write-Host ""
    Write-Host "==> Installer:" -ForegroundColor Cyan
    Get-ChildItem $NsisDir -Filter "*.exe" | ForEach-Object { Write-Host "    $($_.FullName)" }
}
