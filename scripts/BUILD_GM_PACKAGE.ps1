#Requires -Version 5.1
[CmdletBinding()]
param(
  [string]$OutZip = "C:\AegisUltra_GM.zip"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Info($m){ Write-Host "[INFO] $m" }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }
function Fail($m){ Write-Host "[FAIL] $m" -ForegroundColor Red; exit 1 }

$repo = (Get-Location).Path

# ensure release binary exists
if(-not (Test-Path "$repo\dist\windows\aegis_ultra.exe")){
  Warn "dist\\windows\\aegis_ultra.exe not found; run scripts\\BUILD_ALL.ps1 first."
}

$tmp = Join-Path $env:TEMP ("aegis_gm_" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

$include = @(
  "dist",
  "policy",
  "docker",
  "scripts",
  "README.md",
  "QUICKSTART.md",
  "SECURITY.md",
  "LICENSE",
  "VERSION"
)
foreach($x in $include){
  $p = Join-Path $repo $x
  if(Test-Path $p){
    Copy-Item -Recurse -Force -Path $p -Destination (Join-Path $tmp $x)
  }
}

# remove junk
foreach($j in @("node_modules",".git",".cache","target")){
  $jp = Join-Path $tmp $j
  if(Test-Path $jp){ Remove-Item -Recurse -Force $jp }
}

if(Test-Path $OutZip){ Remove-Item $OutZip -Force }
Compress-Archive -Path (Join-Path $tmp "*") -DestinationPath $OutZip

Remove-Item -Recurse -Force $tmp

Info "Gold master package: $OutZip"
