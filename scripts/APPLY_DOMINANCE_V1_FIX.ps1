#Requires -Version 5.1
$ErrorActionPreference="Stop"

function Write-Utf8NoBom {
  param([Parameter(Mandatory=$true)][string]$Path,[Parameter(Mandatory=$true)][string]$Content)
  $enc = New-Object System.Text.UTF8Encoding($false)
  $dir = Split-Path -Parent $Path
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  [System.IO.File]::WriteAllText($Path, $Content, $enc)
}

# 1) Ensure sandbox PATH available if executable not absolute
if (-not $env:AEGIS_SANDBOX_PATH) {
  $env:AEGIS_SANDBOX_PATH = "C:\\Windows\\System32;C:\\Windows"
  Write-Host "Set AEGIS_SANDBOX_PATH for this session."
}

Write-Host "Rebuilding..."
./scripts/BUILD_ALL.ps1
if ($LASTEXITCODE -ne 0) { throw "Build failed" }
Write-Host "✅ Fix patch applied."
