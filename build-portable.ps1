param(
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$PortableRoot = Join-Path $ProjectDir "portable-dist"
$AppDir = Join-Path $PortableRoot "CC Sync"
$ZipPath = Join-Path $PortableRoot "CC Sync portable.zip"
$PythonSource = Resolve-Path (Join-Path $ProjectDir "..\..\python311\Windows-AMD64")
$SourcePython = Join-Path $PythonSource "bin\python.exe"
$ReleaseExe = Join-Path $ProjectDir "src-tauri\target\$Configuration\cc-sync.exe"

Set-Location $ProjectDir

Write-Host "[cc-sync] building release exe..."
npm run tauri:build

if (-not (Test-Path $ReleaseExe)) {
    throw "release exe not found: $ReleaseExe"
}

if (Test-Path $PortableRoot) {
    Remove-Item -LiteralPath $PortableRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $AppDir | Out-Null

Write-Host "[cc-sync] copying app files..."
Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $AppDir "CC Sync.exe")
Copy-Item -LiteralPath (Join-Path $ProjectDir "sync_agents.py") -Destination $AppDir
Copy-Item -LiteralPath (Join-Path $ProjectDir "sync_from_claude.py") -Destination $AppDir
Copy-Item -LiteralPath (Join-Path $ProjectDir "sync_config.py") -Destination $AppDir

Write-Host "[cc-sync] writing default portable config..."
& $SourcePython (Join-Path $ProjectDir "sync_agents.py") `
    --config (Join-Path $AppDir "cc-sync.config.json") `
    --write-default-config `
    --force-overwrite-config

$ReadmeLines = @(
    "CC Sync portable",
    "",
    "Usage:",
    "1. Extract this whole folder.",
    "2. Double-click `"CC Sync.exe`".",
    "3. On first launch, set the workspace root to your project root, for example D:\Dawn\Trunk.",
    "4. Save config, then run sync.",
    "",
    "Notes:",
    "- Do not copy only the exe. Keep the python folder, .py scripts, and cc-sync.config.json next to the exe.",
    "- Runtime paths are relative, so the package can be moved to another machine."
)
$ReadmeLines | Set-Content -LiteralPath (Join-Path $AppDir "README.txt") -Encoding UTF8

Write-Host "[cc-sync] copying embedded Python..."
robocopy $PythonSource (Join-Path $AppDir "python") /E /XD __pycache__ log /XF *.pdb *.pyc | Out-Host
$RoboExit = $LASTEXITCODE
if ($RoboExit -gt 7) {
    throw "robocopy failed with exit code $RoboExit"
}

Write-Host "[cc-sync] creating zip..."
Compress-Archive -LiteralPath $AppDir -DestinationPath $ZipPath -CompressionLevel Optimal

Write-Host "[cc-sync] portable zip ready:"
Write-Host $ZipPath
