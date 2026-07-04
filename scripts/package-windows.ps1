param(
  [string]$OutDir = Join-Path $PSScriptRoot "..\dist\windows-amd64"
)

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$BinName = "coco"
$Version = Select-String -Path (Join-Path $RepoRoot "Cargo.toml") -Pattern "^version\s*=\s*""(.*)""$" | ForEach-Object { $_.Matches.Groups[1].Value } | Select-Object -First 1
$ZipPath = Join-Path $OutDir "${BinName}-${Version}-windows-amd64.zip"
if (-not (Test-Path (Join-Path $RepoRoot "target/release/${BinName}.exe"))) {
  Push-Location $RepoRoot
  cargo build --release --bin $BinName
  Pop-Location
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Compress-Archive -Path (Join-Path $RepoRoot "target/release/${BinName}.exe") -DestinationPath $ZipPath
Write-Host "wrote $ZipPath"
