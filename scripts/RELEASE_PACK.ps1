#Requires -Version 5.1
$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$releaseDir = Join-Path $root "release"
if (-not (Test-Path $releaseDir)) {
  New-Item -ItemType Directory -Path $releaseDir | Out-Null
}

function Require-Path {
  param([string]$Path, [string]$Label)
  if (-not (Test-Path $Path)) {
    throw "Missing required path for release: $Label => $Path"
  }
}

# required inputs
Require-Path (Join-Path $root "dist\\windows\\aegis_ultra.exe") "Windows binary"
Require-Path (Join-Path $root "README.md") "README"
Require-Path (Join-Path $root "CHANGELOG.md") "CHANGELOG"
Require-Path (Join-Path $root "policy\\packs\\policy.json") "policy.json"
Require-Path (Join-Path $root "docker\\docker-compose.yml") "docker compose"

# Windows package
$winZip = Join-Path $releaseDir "AegisUltra_0.1.0_windows.zip"
if (Test-Path $winZip) { Remove-Item $winZip -Force }
$winPaths = @(
  "dist\\windows",
  "README.md",
  "CHANGELOG.md",
  "policy\\packs\\policy.json"
)
Compress-Archive -Path $winPaths -DestinationPath $winZip -Force

# Docker package (docker folder + policy folder + README)
$dockZip = Join-Path $releaseDir "AegisUltra_0.1.0_docker.zip"
if (Test-Path $dockZip) { Remove-Item $dockZip -Force }
$dockPaths = @(
  "docker",
  "policy",
  "README.md"
)
Compress-Archive -Path $dockPaths -DestinationPath $dockZip -Force

# SHA256 sums
$shaPath = Join-Path $releaseDir "sha256sums.txt"
if (Test-Path $shaPath) { Remove-Item $shaPath -Force }
$sb = New-Object System.Text.StringBuilder
foreach ($f in @($winZip, $dockZip)) {
  $h = Get-FileHash -Algorithm SHA256 -Path $f
  [void]$sb.AppendLine("$($h.Hash.ToLower())  $(Split-Path $f -Leaf)")
}
[System.IO.File]::WriteAllText($shaPath, $sb.ToString(), (New-Object System.Text.UTF8Encoding($false)))

Write-Host ("Release artifacts created in {0}:" -f $releaseDir)
Get-ChildItem $releaseDir
