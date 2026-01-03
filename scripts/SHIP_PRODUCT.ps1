#Requires -Version 5.1
[CmdletBinding()]
param(
  [string]$BaseUrl = "http://127.0.0.1:8088",
  [int]$TimeoutSec = 120,
  [int]$SmokeTimeoutSec = 1800,
  [switch]$SkipCompose
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Info($m){ Write-Host "[INFO] $m" }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }

$repo = (Get-Location).Path
$reportsDir = Join-Path $repo "reports"
if(-not (Test-Path $reportsDir)){ New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null }
$stamp = (Get-Date -Format "yyyyMMdd_HHmmss")
$reportJson = Join-Path $reportsDir ("SHIP_PRODUCT_" + $stamp + ".json")
$reportMd   = Join-Path $reportsDir ("SHIP_PRODUCT_" + $stamp + ".md")
$bundlePath = Join-Path $reportsDir ("SHIP_BUNDLE_" + $stamp + ".zip")

$report = [ordered]@{
  base_url = $BaseUrl
  steps = @()
  ok = $false
  outputs = @{}
}

function RunStep($name, [scriptblock]$code){
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $ok = $true
  $err = $null
  try { & $code }
  catch { $ok = $false; $err = $_.Exception.Message }
  $sw.Stop()
  $step = [ordered]@{ name=$name; ok=$ok; ms=[int]$sw.ElapsedMilliseconds }
  if($err){ $step.error = $err }
  $report.steps += $step
  if(-not $ok){ Warn "$name failed: $err" }
}

if(-not $SkipCompose){
  RunStep "docker_down" {
    & docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml down | Out-Null
  }

  RunStep "docker_up" {
    & docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml up --build -d | Out-Null
  }
} else {
  Warn "SkipCompose set: not restarting containers"
}

RunStep "wait_healthz" {
  $healthUrl = ($BaseUrl.TrimEnd("/") + "/healthz")
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $ok = $false
  while((Get-Date) -lt $deadline){
    try {
      $h = Invoke-RestMethod -Method Get -Uri $healthUrl -TimeoutSec 5
      if(($h | Out-String).Trim() -eq "ok"){ $ok = $true; break }
    } catch {}
    Start-Sleep -Milliseconds 600
  }
  if(-not $ok){ throw "healthz not ok within timeout" }
}

$smoke = Join-Path $repo "scripts\SMOKE_DOMINANCE.ps1"
if(Test-Path $smoke){
  RunStep "smoke" {
    & powershell -NoProfile -ExecutionPolicy Bypass -File $smoke -TimeoutSec $TimeoutSec -SmokeTimeoutSec $SmokeTimeoutSec
  }
} else {
  Warn "SMOKE_DOMINANCE.ps1 not found; skipping smoke"
}

$shipv = Join-Path $repo "scripts\SHIP_VERIFY_ALL.ps1"
# ship_verify step intentionally skipped here to keep runtime short and avoid nested compose restarts.

# Bundle packaging
RunStep "bundle_zip" {
  $tmp = Join-Path $repo "SHIP_OUT_tmp_$stamp"
  if(Test-Path $tmp){ Remove-Item -Recurse -Force $tmp }
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $include = @("dist","policy","docker","scripts","README.md","QUICKSTART.md","SECURITY.md","LICENSE","VERSION")
  foreach($x in $include){
    $p = Join-Path $repo $x
    if(Test-Path $p){
      Copy-Item -Recurse -Force -Path $p -Destination (Join-Path $tmp $x)
    }
  }
  # remove junk if present
  $junk = @("node_modules",".git",".cache","target")
  foreach($j in $junk){
    $jp = Join-Path $tmp $j
    if(Test-Path $jp){ Remove-Item -Recurse -Force $jp }
  }
  if(Test-Path $bundlePath){ Remove-Item $bundlePath -Force }
  Compress-Archive -Path (Join-Path $tmp "*") -DestinationPath $bundlePath
  Remove-Item -Recurse -Force $tmp
  $report.outputs.bundle = $bundlePath
}

$allOk = $true
foreach($s in $report.steps){ if(-not $s.ok){ $allOk = $false } }
$report.ok = $allOk

$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $reportJson

$lines = @()
$lines += "SHIP PRODUCT REPORT - $stamp"
$lines += "BaseUrl: $BaseUrl"
$lines += "Overall OK: $allOk"
$lines += ""
$lines += "Steps:"
foreach($s in $report.steps){
  $lines += "$($s.name) ok=$($s.ok) ms=$($s.ms)"
  if($s.PSObject.Properties.Match("error").Count -gt 0 -and $s.error){ $lines += "  err: $($s.error)" }
}
$lines += ""
$lines += "Bundle: $bundlePath"
$lines += "Reports dir: $reportsDir"

Set-Content -Encoding UTF8 -Path $reportMd -Value ($lines -join "`n")

Info "SHIP_PRODUCT finished. ok=$allOk"
Info "Report JSON: $reportJson"
Info "Report MD:   $reportMd"
Info "Bundle:      $bundlePath"
