#Requires -Version 5.1
$ErrorActionPreference = "Stop"

param(
  [string]$BaseUrl = "http://127.0.0.1:8088",
  [int]$TimeoutSec = 180
)

Write-Host "[INFO] Starting stack..."
docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml up --build -d | Out-Null

$health = ($BaseUrl.TrimEnd("/") + "/healthz")
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$ok = $false
while((Get-Date) -lt $deadline){
  try {
    $r = Invoke-RestMethod -Method Get -Uri $health -TimeoutSec 5
    if(($r | Out-String).Trim() -eq "ok"){ $ok = $true; break }
  } catch {}
  Start-Sleep -Milliseconds 700
}
if(-not $ok){ throw "Health not ok at $health within $TimeoutSec s" }

Write-Host "[INFO] Health OK at $health"
try { Start-Process ($BaseUrl.TrimEnd("/") + "/") | Out-Null } catch {}
