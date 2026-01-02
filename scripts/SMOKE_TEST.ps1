#Requires -Version 5.1
$ErrorActionPreference="Stop"
Write-Host "== Health check =="
Invoke-RestMethod -Uri http://127.0.0.1:8088/healthz -Method GET
Write-Host "healthz ok"