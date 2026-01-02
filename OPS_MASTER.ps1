#Requires -Version 5.1
[CmdletBinding()]
param(
  [string]$RepoRoot = (Get-Location).Path,
  [string]$BaseUrl = "http://127.0.0.1:8088",
  [int]$TimeoutSec = 180,
  [switch]$NoRebuild,
  [switch]$NoBrowser,
  [switch]$NoOpenFolder,
  [switch]$SkipSmoke,
  [switch]$SkipGit,
  [switch]$SkipDocker,
  [string]$ComposeFile = "docker/docker-compose.yml",
  [string]$ComposeOverride = "docker/docker-compose.dev-signer.override.yml"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Info($m){ Write-Host "[INFO] $m" }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }

function Run-Capture([string]$Title, [string]$Command, [int]$MaxSeconds = 0){
  $obj = [ordered]@{
    title = $Title
    command = $Command
    ok = $false
    exit_code = $null
    stdout = ""
    stderr = ""
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    ended_utc = $null
    duration_ms = 0
  }
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  try{
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "powershell"
    $psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -Command ""$Command"""
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $p = New-Object System.Diagnostics.Process
    $p.StartInfo = $psi
    [void]$p.Start()
    if($MaxSeconds -gt 0){
      if(-not $p.WaitForExit($MaxSeconds * 1000)){
        try{ $p.Kill() } catch {}
        throw "Timed out after ${MaxSeconds}s"
      }
    } else {
      $p.WaitForExit() | Out-Null
    }
    $obj.stdout = $p.StandardOutput.ReadToEnd()
    $obj.stderr = $p.StandardError.ReadToEnd()
    $obj.exit_code = $p.ExitCode
    $obj.ok = ($p.ExitCode -eq 0)
  } catch {
    $obj.stderr = ($obj.stderr + "`n" + $_.Exception.Message).Trim()
    $obj.ok = $false
  } finally {
    $sw.Stop()
    $obj.ended_utc = (Get-Date).ToUniversalTime().ToString("o")
    $obj.duration_ms = $sw.ElapsedMilliseconds
  }
  return $obj
}

function Ensure-Dir($p){
  if(-not (Test-Path $p)){ New-Item -ItemType Directory -Force -Path $p | Out-Null }
}

function Invoke-Healthz([string]$Url, [int]$MaxWaitSec){
  $deadline = (Get-Date).AddSeconds($MaxWaitSec)
  $lastErr = $null
  while((Get-Date) -lt $deadline){
    try{
      $r = Invoke-RestMethod -Method GET -Uri $Url -TimeoutSec 5
      if(($r | Out-String).Trim() -eq "ok"){ return @{ ok=$true; body="ok" } }
      return @{ ok=$true; body=($r | Out-String).Trim() }
    } catch {
      $lastErr = $_.Exception.Message
      Start-Sleep -Milliseconds 600
    }
  }
  if(-not $lastErr){ $lastErr = "healthz timeout" }
  return @{ ok=$false; body=$lastErr }
}

Info "RepoRoot = $RepoRoot"
Set-Location $RepoRoot
$composeA = Join-Path $RepoRoot $ComposeFile
$composeB = Join-Path $RepoRoot $ComposeOverride
if(-not (Test-Path $composeA)){ throw "Missing compose file: $composeA" }
if(-not (Test-Path $composeB)){ Warn "Override not found (ok if removed): $composeB" }

$reportsDir = Join-Path $RepoRoot "reports"
Ensure-Dir $reportsDir
$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$reportJson = Join-Path $reportsDir "OPS_MASTER_REPORT_$ts.json"
$reportMd   = Join-Path $reportsDir "OPS_MASTER_REPORT_$ts.md"

$smokeRepo = Join-Path $RepoRoot "scripts\SMOKE_DOMINANCE.ps1"
$smokeTemp = Join-Path $env:TEMP "SMOKE_DOMINANCE.ps1"
$smokePath = $null
if(Test-Path $smokeRepo){ $smokePath = $smokeRepo } elseif(Test-Path $smokeTemp){ $smokePath = $smokeTemp }

$report = [ordered]@{
  product = "Aegis Ultra / Dominance"
  run_id = $ts
  repo_root = $RepoRoot
  base_url = $BaseUrl
  started_local = (Get-Date).ToString("o")
  finished_local = $null
  steps = @()
  healthz = $null
  smoke = $null
  docker = $null
  git = $null
  artifacts = @()
  notes = @()
  ok = $false
}

if(-not $SkipDocker){
  if($NoRebuild){
    $report.notes += "NoRebuild enabled; docker compose not restarted."
  } else {
    $report.steps += Run-Capture "docker compose down" "docker compose -f `"$ComposeFile`" -f `"$ComposeOverride`" down" 300
    $report.steps += Run-Capture "docker compose up --build -d" "docker compose -f `"$ComposeFile`" -f `"$ComposeOverride`" up --build -d" 600
    $s3 = Run-Capture "docker compose ps" "docker compose -f `"$ComposeFile`" -f `"$ComposeOverride`" ps" 30
    $report.steps += $s3
    $report.docker = @{ compose_files=@($ComposeFile,$ComposeOverride); ps=$s3.stdout.Trim(); up_ok=$report.steps[-2].ok }
  }
} else { $report.notes += "SkipDocker enabled." }

$healthUrl = ($BaseUrl.TrimEnd("/")) + "/healthz"
$h = Invoke-Healthz $healthUrl $TimeoutSec
$report.healthz = @{ url=$healthUrl; ok=$h.ok; body=$h.body }

if(-not $SkipSmoke -and $smokePath){
  $smokeRun = Join-Path $env:TEMP "SMOKE_DOMINANCE_RUN.ps1"
  Copy-Item $smokePath $smokeRun -Force
  $s = Run-Capture "SMOKE_DOMINANCE.ps1" ("powershell -NoProfile -ExecutionPolicy Bypass -File `"$smokeRun`"") ($TimeoutSec + 120)
  $report.steps += $s
  $report.smoke = @{ path=$smokePath; ok=$s.ok; exit_code=$s.exit_code; stdout=$s.stdout; stderr=$s.stderr }
  $bundleLine = ($s.stdout -split "`r?`n") | Where-Object { $_ -match "Bundle saved to" } | Select-Object -First 1
  if($bundleLine){
    $p = ($bundleLine -replace "^.*Bundle saved to\s+","").Trim()
    if(Test-Path $p){ $report.artifacts += @{ kind="bundle_zip"; path=$p } }
  }
} else { $report.notes += "Smoke skipped or script missing." }

if(-not $SkipGit){
  try{
    & git rev-parse --is-inside-work-tree 2>$null | Out-Null
    if($LASTEXITCODE -eq 0){
      $st = Run-Capture "git status (porcelain)" "git status --porcelain" 20
      $hd = Run-Capture "git head" "git rev-parse HEAD" 20
      $br = Run-Capture "git branch" "git branch --show-current" 20
      $tg = Run-Capture "git tags" "git tag --list --sort=-creatordate | Select-Object -First 20" 20
      $report.steps += $st; $report.steps += $hd; $report.steps += $br; $report.steps += $tg
      $report.git = @{ head=$hd.stdout.Trim(); branch=$br.stdout.Trim(); status_porcelain=$st.stdout.Trim(); recent_tags=$tg.stdout.Trim() }
    } else { $report.notes += "Git not available." }
  } catch { $report.notes += "Git not available: " + $_.Exception.Message }
} else { $report.notes += "SkipGit enabled." }

$okHealth = $report.healthz.ok
$okSmoke  = ($SkipSmoke -or ($report.smoke -and $report.smoke.ok))
$okDocker = ($SkipDocker -or $NoRebuild -or ($report.docker -and $report.docker.up_ok))
$report.ok = ($okHealth -and $okSmoke -and $okDocker)
$report.finished_local = (Get-Date).ToString("o")

Ensure-Dir (Split-Path -Parent $reportJson)
$report | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 $reportJson

$md = @()
$md += "# OPS MASTER REPORT - $ts"
$md += ""
$md += "- Repo: $RepoRoot"
$md += "- Base URL: $BaseUrl"
$md += "- Healthz: $($report.healthz.ok) ($healthUrl) -- $($report.healthz.body)"
$md += "- Smoke: " + ($(if($SkipSmoke){"SKIPPED"} elseif($report.smoke.ok){"OK"} else {"FAIL"}))
$md += "- Docker: " + ($(if($SkipDocker){"SKIPPED"} elseif($NoRebuild){"SKIPPED (NoRebuild)"} elseif($report.docker -and $report.docker.up_ok){"OK"} else {"FAIL"}))
$md += "- Overall: " + ($(if($report.ok){"GREEN"} else {"NOT GREEN"}))
$md += ""
$md += "## URLs"
$md += "- $healthUrl"
$md += "- $($BaseUrl.TrimEnd('/'))/version"
$md += "- $($BaseUrl.TrimEnd('/'))/v1/aegis/bundle/<request_id>"
$md += ""
$md += "## Notes"
foreach($n in $report.notes){ $md += "- " + $n }
$md += ""
$md += "## Git"
if($report.git){
  $md += "- Branch: $($report.git.branch)"
  $md += "- HEAD: $($report.git.head)"
  $md += "Status:"
  $md += $report.git.status_porcelain
} else { $md += "_Git not captured._" }
$md += ""
$md += "## Docker ps"
if($report.docker){
  $md += $report.docker.ps
} else { $md += "_Docker not captured._" }
$md += ""
$md += "## Smoke stdout"
if($report.smoke){
  $md += $report.smoke.stdout.TrimEnd()
}
$md += ""
$md += "## Steps"
foreach($s in $report.steps){
  $md += "### " + $s.title
  $md += "- ok: " + $s.ok + "  exit: " + $s.exit_code + "  ms: " + $s.duration_ms
  if($s.stdout){ $md += $s.stdout.TrimEnd() }
  if($s.stderr){ $md += "--- stderr ---`n" + $s.stderr.TrimEnd() }
  $md += ""
}

$md -join "`n" | Set-Content -Encoding UTF8 $reportMd

Info "Report written:"
Info "  $reportJson"
Info "  $reportMd"

if(-not $NoOpenFolder){
  try{ Invoke-Item $reportsDir } catch {}
}
if(-not $NoBrowser){
  try{ Start-Process ($BaseUrl.TrimEnd("/") + "/healthz") | Out-Null } catch {}
}

if($report.ok){
  Info "OPS MASTER: GREEN"
  exit 0
} else {
  Warn "OPS MASTER: NOT GREEN (see report)"
  exit 2
}
