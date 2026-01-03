#Requires -Version 5.1
[CmdletBinding()]
param(
  [string]$BaseUrl = "http://127.0.0.1:8088",
  [int]$TimeoutSec = 60
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Info($m){ Write-Host "[INFO] $m" }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }
function Fail($m){ Write-Host "[FAIL] $m" -ForegroundColor Red; exit 1 }

# 0) Docker
if(-not (Get-Command docker -ErrorAction SilentlyContinue)){
  Fail "Docker not found on PATH."
}
try { docker info | Out-Null } catch { Fail "Docker not running: $($_.Exception.Message)" }

# 1) Compose ps
try {
  Info "Docker compose ps:"
  docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml ps
} catch { Warn "Compose ps failed: $($_.Exception.Message)" }

# 2) Port check
$portInUse = Get-NetTCPConnection -LocalPort 8088 -ErrorAction SilentlyContinue
if($portInUse){
  Fail "Port 8088 is already in use. Stop the other service or change AEGIS_BIND."
}

# 3) Health / ready
$healthUrl = ($BaseUrl.TrimEnd("/") + "/healthz")
$readyUrl  = ($BaseUrl.TrimEnd("/") + "/readyz")
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$healthOk = $false
while((Get-Date) -lt $deadline){
  try {
    $h = Invoke-RestMethod -Method Get -Uri $healthUrl -TimeoutSec 5
    if(($h | Out-String).Trim() -eq "ok"){ $healthOk = $true; break }
  } catch {}
  Start-Sleep -Milliseconds 500
}
if(-not $healthOk){ Fail "healthz not ok within $TimeoutSec seconds at $healthUrl" }
Info "healthz ok"

$readyOk = $false
try {
  $r = Invoke-RestMethod -Method Get -Uri $readyUrl -TimeoutSec 5
  if($r.ok -eq $true){ $readyOk = $true }
} catch {}
if(-not $readyOk){ Warn "readyz not ok (OPA or upstream may be unreachable); continuing." }
else { Info "readyz ok" }

# 4) Auth/token hint
if(-not $env:AEGIS_UI_TOKEN){ Warn "AEGIS_UI_TOKEN not set (UI/API open in local-only mode)." }

Info "Preflight completed."
