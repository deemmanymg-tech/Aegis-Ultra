#Requires -Version 5.1
[CmdletBinding()]
param(
  [string]$RepoRoot = (Resolve-Path ".").Path,
  [string]$BaseUrl  = "http://127.0.0.1:8088",
  [int]$TimeoutSec  = 120,
  [int]$SmokeTimeoutSec = 1800,
  [switch]$SkipDockerRestart,
  [switch]$SkipRustChecks
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Info($m){ Write-Host "[INFO] $m" }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }

function Exec([string]$cmd){
  Info $cmd
  $p = Start-Process -FilePath "powershell" -ArgumentList @("-NoProfile","-ExecutionPolicy","Bypass","-Command",$cmd) -Wait -PassThru
  if($p.ExitCode -ne 0){ throw "Command failed ($($p.ExitCode)): $cmd" }
}

function TryExec([string]$cmd){
  try { Exec $cmd; $true } catch { Warn $_.Exception.Message; $false }
}

$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$reportsDir = Join-Path $RepoRoot "reports"
if(!(Test-Path $reportsDir)){ New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null }

$shipJson = Join-Path $reportsDir ("DOMINANCE_SHIP_REPORT_{0}.json" -f $ts)
$shipMd   = Join-Path $reportsDir ("DOMINANCE_SHIP_REPORT_{0}.md" -f $ts)
$smoke    = Join-Path $RepoRoot "scripts\SMOKE_DOMINANCE.ps1"
$verify   = Join-Path $RepoRoot "VERIFY_DOMINANCE_RELEASE.ps1"

if(!(Test-Path $smoke)){ throw "Missing: $smoke" }
if(!(Test-Path $verify)){ Warn "Missing VERIFY_DOMINANCE_RELEASE.ps1 (will skip)" }

Info "RepoRoot = $RepoRoot"
Info "BaseUrl  = $BaseUrl"

if(!$SkipDockerRestart){
  Info "Restarting compose (dev-signer override) ..."
  Exec "cd `"$RepoRoot`"; docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml down"
  Exec "cd `"$RepoRoot`"; docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml up --build -d"
}

Info "Waiting for /healthz ..."
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$healthOk = $false
while((Get-Date) -lt $deadline){
  try{
    $h = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd("/") + "/healthz") -TimeoutSec 5
    if($h -eq "ok"){ $healthOk = $true; break }
  } catch {}
  Start-Sleep -Seconds 2
}
if(!$healthOk){ throw "Healthz never became ok within ${TimeoutSec}s" }
Info "healthz => ok"

Info "Running SMOKE_DOMINANCE.ps1 ..."
$smokeOut = & powershell -NoProfile -ExecutionPolicy Bypass -File $smoke 2>&1 | Out-String
Info "Smoke complete."

$requestId = $null
if($smokeOut -match '"request_id"\s*:\s*"([0-9a-fA-F-]{36})"'){
  $requestId = $Matches[1]
  Info ("request_id = " + $requestId)
} else {
  Warn "Could not parse request_id from smoke output. Evidence checks will be limited."
}

$artifactsDir = Join-Path $RepoRoot "artifacts"
$evidence = @{}
if($requestId){
  $reqDir = Join-Path $artifactsDir $requestId
  $stdoutPath = Join-Path $reqDir "stdout.txt"
  $stderrPath = Join-Path $reqDir "stderr.txt"
  $decisionPath = Join-Path $reqDir "decision.json"
  $evidence.reqDir = $reqDir
  $evidence.stdout_exists  = Test-Path $stdoutPath
  $evidence.stderr_exists  = Test-Path $stderrPath
  $evidence.decision_exists= Test-Path $decisionPath
  if(!$evidence.stdout_exists){ Warn "Missing: $stdoutPath" }
  if(!$evidence.stderr_exists){ Warn "Missing: $stderrPath" }
  if(!$evidence.decision_exists){ Warn "Missing: $decisionPath" }
}

$rust = @{
  fmt   = $null
  clippy= $null
}
if(!$SkipRustChecks){
  Info "Running cargo fmt / clippy (best effort) ..."
  $rust.fmt    = TryExec "cd `"$RepoRoot`"; cargo fmt --all -- --check"
  $rust.clippy = TryExec "cd `"$RepoRoot`"; cargo clippy -- -D warnings"
} else {
  Info "SkipRustChecks enabled."
}

$releaseReport = $null
if(Test-Path $verify){
  Info "Running VERIFY_DOMINANCE_RELEASE.ps1 (full report) ..."
  $null = & powershell -NoProfile -ExecutionPolicy Bypass -File $verify 2>&1 | Out-String
  $latest = Get-ChildItem -Path $reportsDir -Filter "DOMINANCE_RELEASE_REPORT_*.json" -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if($latest){ $releaseReport = $latest.FullName }
}

$shipObj = [ordered]@{
  timestamp = $ts
  repo_root = $RepoRoot
  base_url  = $BaseUrl
  healthz_ok= $healthOk
  request_id= $requestId
  evidence  = $evidence
  rust_checks = $rust
  release_report_json = $releaseReport
  smoke_stdout = $smokeOut
}

$shipObj | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 -Path $shipJson

$md = @()
$md += "# Dominance Ship Report ($ts)"
$md += ""
$md += "- Base URL: $BaseUrl"
$md += "- healthz: ok"
if($requestId){ $md += "- request_id: $requestId" }
$md += ""
$md += "## Smoke output"
$md += "```"
$md += ($smokeOut.TrimEnd())
$md += "```"
$md += ""
$md += "## Evidence"
if($requestId){
  $md += "- artifacts/$requestId exists: $([bool](Test-Path (Join-Path $artifactsDir $requestId)))"
  $md += "- stdout.txt: $($evidence.stdout_exists)"
  $md += "- stderr.txt: $($evidence.stderr_exists)"
  $md += "- decision.json: $($evidence.decision_exists)"
} else {
  $md += "- request_id not detected; evidence checks skipped."
}
$md += ""
$md += "## Rust checks"
$md += "- cargo fmt --check: $($rust.fmt)"
$md += "- cargo clippy -D warnings: $($rust.clippy)"
$md += ""
if($releaseReport){
  $md += "## Release verifier"
  $md += "- Latest release report: $releaseReport"
  $md += ""
}
$md += "## Outputs"
$md += "- JSON: $shipJson"
$md += "- MD:   $shipMd"

$md -join "`r`n" | Set-Content -Encoding UTF8 -Path $shipMd

Info "Ship report written:"
Info " - $shipJson"
Info " - $shipMd"
