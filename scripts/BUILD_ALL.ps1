#Requires -Version 5.1
$ErrorActionPreference="Stop"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "ERROR: cargo not found. Install Rust via rustup then reopen PowerShell."
  Write-Host "Install: https://www.rust-lang.org/tools/install"
  exit 1
}

Write-Host "== Build Rust release (Windows binary) =="
cargo build --release

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot "..\\target" }
$exePath = Join-Path $targetDir "release\\aegis_ultra.exe"

New-Item -ItemType Directory -Force -Path .\dist\windows | Out-Null
if (-not (Test-Path $exePath)) {
  throw "Expected binary not found at $exePath"
}
Copy-Item $exePath .\dist\windows\ -Force
Copy-Item .\policy\packs\policy.json .\dist\windows\policy.json -Force

Write-Host "== Build Docker image =="
docker build -f .\docker\Dockerfile -t aegis-ultra:0.1.0 .

Write-Host "Build complete."
