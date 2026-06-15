param(
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$PortableRoot = Join-Path $ProjectDir "portable-dist"
$AppDir = Join-Path $PortableRoot "CC Sync"
$Version = (node -p "require('./package.json').version").Trim()
$ZipPath = Join-Path $PortableRoot "CC.Sync_${Version}_x64.portable.zip"
$ReleaseExe = Join-Path $ProjectDir "src-tauri\target\$Configuration\cc-sync.exe"

Set-Location $ProjectDir

Write-Host "[cc-sync] building release exe..."
npm run tauri:build
if ($LASTEXITCODE -ne 0) {
    throw "tauri build failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path $ReleaseExe)) {
    throw "release exe not found: $ReleaseExe"
}

if (Test-Path $PortableRoot) {
    Remove-Item -LiteralPath $PortableRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $AppDir | Out-Null

Write-Host "[cc-sync] copying app files..."
Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $AppDir "CC Sync.exe")

Write-Host "[cc-sync] writing default portable config..."
Copy-Item -LiteralPath (Join-Path $ProjectDir "cc-sync.config.example.json") -Destination (Join-Path $AppDir "cc-sync.config.json")

$ReadmeLines = @(
    "CC Sync 便携版",
    "",
    "一句话：一处维护，按需同步；自己管理实际在用的 skills、docs 和说明文件。",
    "",
    "使用方式：",
    "1. 保持整个文件夹完整，不要只复制 `"CC Sync.exe`"。文件夹放在哪里都可以。",
    "2. 双击 `"CC Sync.exe`" 启动。",
    "3. 首次启动选择要同步的项目工作区（你的项目根目录）。",
    "4. 添加说明文件、skills 目录、docs 目录等输入源。",
    "5. 勾选目标端点，例如 Codex / Gemini / Claude Code。",
    "6. 先看同步预览，确认路径和数量正确后再执行。",
    "",
    "目录要求：",
    "- CC Sync.exe 和 cc-sync.config.json 放在同一目录即可。",
    "- cc-sync.config.json 是本机配置，可以按实际使用习惯调整。",
    "- cc-sync.state.json 是本机同步状态，用来识别 CC Sync 自己管理过的 skills/docs。",
    "- 多个项目工作区可以在界面里保存并切换。",
    "",
    "说明：",
    "- 同步引擎已内置在 CC Sync.exe 中，不需要 Python。"
)
$ReadmeLines | Set-Content -LiteralPath (Join-Path $AppDir "README.txt") -Encoding UTF8

Write-Host "[cc-sync] creating zip..."
Compress-Archive -LiteralPath $AppDir -DestinationPath $ZipPath -CompressionLevel Optimal

Write-Host "[cc-sync] writing SHA256SUMS..."
$ShaFile = Join-Path $PortableRoot "SHA256SUMS-windows-${Version}.txt"
$Hash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLower()
"$Hash  $(Split-Path -Leaf $ZipPath)" | Set-Content -LiteralPath $ShaFile -Encoding ASCII

Write-Host "[cc-sync] portable zip ready:"
Write-Host $ZipPath
Write-Host $ShaFile
