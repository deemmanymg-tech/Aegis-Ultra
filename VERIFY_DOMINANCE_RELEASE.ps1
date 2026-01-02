#Requires -Version 5.1
<#
VERIFY_DOMINANCE_RELEASE.ps1
Release-grade verifier for Dominance v1 (or any commit).
#>

[CmdletBinding()]
param(
  [string]$RepoRoot = (Get-Location).Path,
  [string]$BaseUrl = "http://127.0.0.1:8000",
  [string]$ComposeFile = ".\docker\docker-compose.yml",
  [string]$OverrideFile = ".\docker\docker-compose.dev-signer.override.yml",
  [switch]$RunDocker,
  [switch]$RunSmoke,
  [switch]$RunRuntimeHttpChecks,
  [switch]$RunRustTests,
  [switch]$SkipRust,
  [switch]$SkipGit,
  [int]$SmokeTimeoutSec = 1800,
  [string]$SmokeScriptPath = (Join-Path $env:TEMP "SMOKE_DOMINANCE.ps1"),
  [string]$ReportDir = ".\reports",
  [string]$ExpectedTag = "v1.0.0"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function NowStamp { Get-Date -Format "yyyy-MM-dd HH:mm:ss.fff zzz" }
function FileStamp { Get-Date -Format "yyyyMMdd_HHmmss" }

function New-Result([string]$Name){
  [PSCustomObject]@{
    name = $Name
    ok = $true
    details = @()
    evidence = @()
    started_at = (NowStamp)
    ended_at = $null
    duration_ms = 0
  }
}

function Add-Detail($res, [string]$line){ $res.details += $line }
function Add-Evidence($res, [string]$line){ $res.evidence += $line }
function Fail-Result($res, [string]$line){ $res.ok = $false; Add-Detail $res $line }

function Run-Cmd {
  param(
    [Parameter(Mandatory=$true)][string]$Title,
    [Parameter(Mandatory=$true)][string]$Exe,
    [string[]]$Args = @(),
    [int]$TimeoutSec = 600,
    [switch]$AllowFail
  )
  $p = New-Object System.Diagnostics.Process
  $p.StartInfo.FileName = $Exe
  $p.StartInfo.Arguments = ($Args -join " ")
  $p.StartInfo.RedirectStandardOutput = $true
  $p.StartInfo.RedirectStandardError = $true
  $p.StartInfo.UseShellExecute = $false
  $p.StartInfo.CreateNoWindow = $true

  $null = $p.Start()
  if(-not $p.WaitForExit($TimeoutSec * 1000)){
    try { $p.Kill() } catch {}
    throw "Command timed out: $Title ($Exe $($Args -join ' '))"
  }
  $stdout = $p.StandardOutput.ReadToEnd()
  $stderr = $p.StandardError.ReadToEnd()
  $code = $p.ExitCode

  [PSCustomObject]@{
    title = $Title
    exe = $Exe
    args = $Args
    exit_code = $code
    stdout = $stdout
    stderr = $stderr
    ok = ($code -eq 0) -or $AllowFail.IsPresent
  }
}

function Ensure-Dir([string]$Path){
  if(!(Test-Path $Path)){ New-Item -ItemType Directory -Force -Path $Path | Out-Null }
}

function Safe-ReadAllText([string]$Path){
  if(!(Test-Path $Path)){ return $null }
  return Get-Content -Raw -Encoding UTF8 $Path
}

function To-JsonPretty($obj){
  return ($obj | ConvertTo-Json -Depth 20)
}

$start = Get-Date
Push-Location $RepoRoot

Ensure-Dir $ReportDir
$stamp = FileStamp
$mdPath = Join-Path $ReportDir "DOMINANCE_RELEASE_REPORT_$stamp.md"
$jsonPath = Join-Path $ReportDir "DOMINANCE_RELEASE_REPORT_$stamp.json"

$report = [ordered]@{
  meta = [ordered]@{
    generated_at = (NowStamp)
    repo_root = $RepoRoot
    base_url = $BaseUrl
    compose_file = $ComposeFile
    override_file = $OverrideFile
    expected_tag = $ExpectedTag
    hostname = $env:COMPUTERNAME
    username = $env:USERNAME
    pwsh = $PSVersionTable.PSVersion.ToString()
  }
  summary = [ordered]@{
    ok = $true
    failed_checks = @()
    warnings = @()
  }
  checks = @()
}

if(-not $SkipGit){
  $res = New-Result "git_hygiene"
  $t0 = [DateTime]::UtcNow

  try {
    $gitVer = Run-Cmd -Title "git --version" -Exe "git" -Args @("--version") -TimeoutSec 60
    Add-Evidence $res ("git: " + ($gitVer.stdout.Trim()))

    $head = Run-Cmd -Title "git rev-parse HEAD" -Exe "git" -Args @("rev-parse","HEAD") -TimeoutSec 60
    Add-Detail $res ("HEAD: " + ($head.stdout.Trim()))

    $branch = Run-Cmd -Title "git rev-parse --abbrev-ref HEAD" -Exe "git" -Args @("rev-parse","--abbrev-ref","HEAD") -TimeoutSec 60
    Add-Detail $res ("Branch: " + ($branch.stdout.Trim()))

    $status = Run-Cmd -Title "git status --porcelain" -Exe "git" -Args @("status","--porcelain") -TimeoutSec 60 -AllowFail
    if($status.stdout.Trim().Length -ne 0){
      Fail-Result $res "Working tree is not clean (git status --porcelain not empty)."
      Add-Evidence $res $status.stdout.Trim()
    } else {
      Add-Detail $res "Working tree clean."
    }

    $tracked = Run-Cmd -Title "git ls-files override" -Exe "git" -Args @("ls-files",$OverrideFile) -TimeoutSec 60 -AllowFail
    if($tracked.stdout.Trim().Length -ne 0){
      Fail-Result $res "Dev-signer override file is TRACKED by git. It must be untracked."
      Add-Evidence $res $tracked.stdout.Trim()
    } else {
      Add-Detail $res "Dev-signer override not tracked."
    }

    $gi = Safe-ReadAllText (Join-Path $RepoRoot ".gitignore")
    if($gi -and $gi -match [regex]::Escape(($OverrideFile -replace '^\.\[\\/]', '').Replace('\','/'))){
      Add-Detail $res ".gitignore includes dev-signer override (path match)."
    } elseif($gi -and $gi -match "docker-compose\.dev-signer\.override\.yml"){
      Add-Detail $res ".gitignore includes dev-signer override (filename match)."
    } else {
      Fail-Result $res ".gitignore does not appear to include dev-signer override ignore rule."
    }

    if($ExpectedTag){
      $tag = Run-Cmd -Title "git tag -l" -Exe "git" -Args @("tag","-l",$ExpectedTag) -TimeoutSec 60 -AllowFail
      if($tag.stdout.Trim().Length -eq 0){
        Add-Detail $res "Tag $ExpectedTag not found (ok if you haven't tagged yet)."
        $report.summary.warnings += "Tag $ExpectedTag not found yet."
      } else {
        $tagHead = Run-Cmd -Title "git rev-list -n 1 tag" -Exe "git" -Args @("rev-list","-n","1",$ExpectedTag) -TimeoutSec 60 -AllowFail
        $head2 = (Run-Cmd -Title "git rev-parse HEAD (again)" -Exe "git" -Args @("rev-parse","HEAD") -TimeoutSec 60).stdout.Trim()
        if($tagHead.stdout.Trim() -ne $head2){
          Fail-Result $res "Tag $ExpectedTag does not point to HEAD."
          Add-Detail $res ("TagCommit: " + $tagHead.stdout.Trim())
          Add-Detail $res ("HEADCommit: " + $head2)
        } else {
          Add-Detail $res "Tag $ExpectedTag points to HEAD."
        }
      }
    }
  } catch {
    Fail-Result $res ("Exception: " + $_.Exception.Message)
  }

  $res.ended_at = (NowStamp)
  $res.duration_ms = [int](([DateTime]::UtcNow - $t0).TotalMilliseconds)
  $report.checks += $res
  if(-not $res.ok){ $report.summary.ok = $false; $report.summary.failed_checks += $res.name }
}

if(-not $SkipRust){
  $res = New-Result "rust_quality"
  $t0 = [DateTime]::UtcNow

  try {
    $cargoVer = Run-Cmd -Title "cargo --version" -Exe "cargo" -Args @("--version") -TimeoutSec 60
    Add-Evidence $res ("cargo: " + $cargoVer.stdout.Trim())

    $fmt = Run-Cmd -Title "cargo fmt --check" -Exe "cargo" -Args @("fmt","--check") -TimeoutSec 600 -AllowFail
    if($fmt.exit_code -ne 0){
      Fail-Result $res "cargo fmt --check failed."
      Add-Evidence $res ($fmt.stdout + "`n" + $fmt.stderr).Trim()
    } else {
      Add-Detail $res "cargo fmt --check: OK"
    }

    $clippy = Run-Cmd -Title "cargo clippy -- -D warnings" -Exe "cargo" -Args @("clippy","--","-D","warnings") -TimeoutSec 1800 -AllowFail
    if($clippy.exit_code -ne 0){
      Fail-Result $res "cargo clippy -- -D warnings failed."
      Add-Evidence $res ($clippy.stdout + "`n" + $clippy.stderr).Trim()
    } else {
      Add-Detail $res "cargo clippy -- -D warnings: OK"
    }

    if($RunRustTests){
      $test = Run-Cmd -Title "cargo test" -Exe "cargo" -Args @("test") -TimeoutSec 3600 -AllowFail
      if($test.exit_code -ne 0){
        Fail-Result $res "cargo test failed."
        Add-Evidence $res ($test.stdout + "`n" + $test.stderr).Trim()
      } else {
        Add-Detail $res "cargo test: OK"
      }
    } else {
      Add-Detail $res "cargo test: skipped (enable with -RunRustTests)"
    }
  } catch {
    Fail-Result $res ("Exception: " + $_.Exception.Message)
  }

  $res.ended_at = (NowStamp)
  $res.duration_ms = [int](([DateTime]::UtcNow - $t0).TotalMilliseconds)
  $report.checks += $res
  if(-not $res.ok){ $report.summary.ok = $false; $report.summary.failed_checks += $res.name }
}

if($RunDocker){
  $res = New-Result "docker_sanity"
  $t0 = [DateTime]::UtcNow

  try {
    $dock = Run-Cmd -Title "docker version" -Exe "docker" -Args @("version") -TimeoutSec 120 -AllowFail
    if($dock.exit_code -ne 0){
      Fail-Result $res "docker is not available or not running."
      Add-Evidence $res ($dock.stdout + "`n" + $dock.stderr).Trim()
    } else {
      Add-Detail $res "docker: OK"
    }

    $cfg = Run-Cmd -Title "docker compose config" -Exe "docker" -Args @("compose","-f",$ComposeFile,"-f",$OverrideFile,"config") -TimeoutSec 300 -AllowFail
    if($cfg.exit_code -ne 0){
      Fail-Result $res "docker compose config failed (check compose files)."
      Add-Evidence $res ($cfg.stdout + "`n" + $cfg.stderr).Trim()
    } else {
      Add-Detail $res "docker compose config: OK"
    }

    $ps = Run-Cmd -Title "docker compose ps" -Exe "docker" -Args @("compose","-f",$ComposeFile,"-f",$OverrideFile,"ps") -TimeoutSec 120 -AllowFail
    Add-Evidence $res ("compose ps:`n" + ($ps.stdout.Trim()))

    $logs = Run-Cmd -Title "docker compose logs -n 120" -Exe "docker" -Args @("compose","-f",$ComposeFile,"-f",$OverrideFile,"logs","-n","120") -TimeoutSec 120 -AllowFail
    Add-Evidence $res ("compose logs (tail):`n" + ($logs.stdout.Trim()))
  } catch {
    Fail-Result $res ("Exception: " + $_.Exception.Message)
  }

  $res.ended_at = (NowStamp)
  $res.duration_ms = [int](([DateTime]::UtcNow - $t0).TotalMilliseconds)
  $report.checks += $res
  if(-not $res.ok){ $report.summary.ok = $false; $report.summary.failed_checks += $res.name }
}

if($RunRuntimeHttpChecks){
  $res = New-Result "runtime_http_checks"
  $t0 = [DateTime]::UtcNow

  try {
    $healthUrl = ($BaseUrl.TrimEnd("/")) + "/healthz"
    Add-Detail $res ("GET " + $healthUrl)

    $healthOk = $false
    try {
      $r = Invoke-WebRequest -UseBasicParsing -Uri $healthUrl -Method GET -TimeoutSec 10
      $healthOk = ($r.StatusCode -eq 200 -and ($r.Content -match "ok"))
      Add-Evidence $res ("healthz status=" + $r.StatusCode + " body=" + ($r.Content.Trim()))
    } catch {
      Add-Evidence $res ("healthz error: " + $_.Exception.Message)
    }
    if(-not $healthOk){ Fail-Result $res "healthz did not return 200 with body containing 'ok'." }
    else { Add-Detail $res "healthz: OK" }

    $denyUrl = ($BaseUrl.TrimEnd("/")) + "/v1/tools/commit"
    Add-Detail $res ("POST " + $denyUrl + " (deny probe)")
    $denyObserved = $false
    try {
      $body = @{ request_id="bogus"; prepare_digest="bogus"; approval=$null } | ConvertTo-Json
      $r2 = Invoke-WebRequest -UseBasicParsing -Uri $denyUrl -Method POST -ContentType "application/json" -Body $body -TimeoutSec 10
      Add-Evidence $res ("deny probe status=" + $r2.StatusCode + " body=" + ($r2.Content.Trim()))
      if($r2.StatusCode -ge 400){ $denyObserved = $true }
    } catch {
      $msg = $_.Exception.Message
      Add-Evidence $res ("deny probe error (often expected): " + $msg)
      if($msg -match "\b403\b" -or $msg -match "\b400\b"){ $denyObserved = $true }
    }
    if(-not $denyObserved){
      Fail-Result $res "Deny probe did not observe a 4xx response (may need endpoint adjustment)."
      $report.summary.warnings += "Runtime deny probe endpoint may differ; adjust -BaseUrl or the deny probe route in script."
    } else {
      Add-Detail $res "deny probe: observed 4xx (OK)"
    }
  } catch {
    Fail-Result $res ("Exception: " + $_.Exception.Message)
  }

  $res.ended_at = (NowStamp)
  $res.duration_ms = [int](([DateTime]::UtcNow - $t0).TotalMilliseconds)
  $report.checks += $res
  if(-not $res.ok){ $report.summary.ok = $false; $report.summary.failed_checks += $res.name }
}

if($RunSmoke){
  $res = New-Result "smoke_dominance"
  $t0 = [DateTime]::UtcNow

  try {
    if(!(Test-Path $SmokeScriptPath)){
      Fail-Result $res "Smoke script not found."
      Add-Detail $res ("SmokeScriptPath: " + $SmokeScriptPath)
    } else {
      Add-Detail $res ("Running smoke: " + $SmokeScriptPath)

      $sm = Run-Cmd -Title "SMOKE_DOMINANCE.ps1" -Exe "powershell" -Args @("-ExecutionPolicy","Bypass","-File",$SmokeScriptPath) -TimeoutSec $SmokeTimeoutSec -AllowFail
      Add-Evidence $res ("exit_code=" + $sm.exit_code)
      Add-Evidence $res ("stdout:`n" + ($sm.stdout.Trim()))
      if($sm.stderr.Trim().Length -gt 0){ Add-Evidence $res ("stderr:`n" + ($sm.stderr.Trim())) }

      $out = ($sm.stdout + "`n" + $sm.stderr)
      $expect = @(
        @{ name="prepare_ok"; pattern='tool prepare|[3/6].*prepare' },
        @{ name="commit_without_approval_denied"; pattern='(?s)commit without approval.*Denied as expected' },
        @{ name="commit_with_approval_ok"; pattern='(?s)commit with approval.*exit_code.*0' },
        @{ name="bundle_saved"; pattern='bundle.*(saved|downloaded|\.zip)' }
      )

      foreach($e in $expect){
        if($out -match $e.pattern){
          Add-Detail $res ("Smoke milestone OK: " + $e.name)
        } else {
          Fail-Result $res ("Smoke milestone missing: " + $e.name + " (pattern: " + $e.pattern + ")")
        }
      }

      if($sm.exit_code -ne 0){
        Fail-Result $res ("Smoke script exit_code != 0 (" + $sm.exit_code + ")")
      }
    }
  } catch {
    Fail-Result $res ("Exception: " + $_.Exception.Message)
  }

  $res.ended_at = (NowStamp)
  $res.duration_ms = [int](([DateTime]::UtcNow - $t0).TotalMilliseconds)
  $report.checks += $res
  if(-not $res.ok){ $report.summary.ok = $false; $report.summary.failed_checks += $res.name }
}

$end = Get-Date
$elapsedMs = [int](($end - $start).TotalMilliseconds)

$report.meta.total_duration_ms = $elapsedMs
$report.meta.completed_at = (NowStamp)

$lines = New-Object System.Collections.Generic.List[string]
$nl = [Environment]::NewLine
$lines.Add("Dominance Release Verification Report")
$lines.Add("")
$lines.Add("Generated: " + $report.meta.generated_at)
$lines.Add("Completed: " + $report.meta.completed_at)
$lines.Add("RepoRoot: " + $report.meta.repo_root)
$lines.Add("BaseUrl: " + $report.meta.base_url)
$lines.Add("Compose: " + $report.meta.compose_file + " + " + $report.meta.override_file)
$lines.Add("ExpectedTag: " + $report.meta.expected_tag)
$lines.Add("Total Duration: " + $elapsedMs + " ms")
$lines.Add("")
$lines.Add("Summary")
$lines.Add("")
$lines.Add("Overall: " + ($(if($report.summary.ok){"PASS"} else {"FAIL"})))
if($report.summary.failed_checks.Count -gt 0){
  $lines.Add("Failed Checks: " + ($report.summary.failed_checks -join ", "))
}
if($report.summary.warnings.Count -gt 0){
  $lines.Add("Warnings:")
  foreach($w in $report.summary.warnings){ $lines.Add(" - " + $w) }
}
$lines.Add("")
$lines.Add("Checks")
$lines.Add("")

foreach($c in $report.checks){
  $lines.Add("Check: " + $c.name)
  $lines.Add("Status: " + ($(if($c.ok){"PASS"} else {"FAIL"})))
  $lines.Add("Started: " + $c.started_at)
  $lines.Add("Ended: " + $c.ended_at)
  $lines.Add("Duration: " + $c.duration_ms + " ms")
  if($c.details.Count -gt 0){
    $lines.Add("Details:")
    foreach($d in $c.details){ $lines.Add(" - " + $d) }
  }
  if($c.evidence.Count -gt 0){
    $lines.Add("Evidence:")
    $ev = ($c.evidence -join $nl)
    if($ev.Length -gt 20000){ $ev = $ev.Substring(0,20000) + $nl + "[truncated]" }
    foreach($line in $ev.Split($nl)){ $lines.Add(" > " + $line) }
  }
  $lines.Add("")
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText($mdPath, ($lines -join $nl), $utf8NoBom)
[IO.File]::WriteAllText($jsonPath, (To-JsonPretty $report), $utf8NoBom)

Write-Host ""
Write-Host "=== Dominance Release Verification ==="
Write-Host ("Overall: " + ($(if($report.summary.ok){"PASS"} else {"FAIL"})))
Write-Host ("Report (MD):  $mdPath")
Write-Host ("Report (JSON): $jsonPath")
Write-Host ""

Pop-Location

if(-not $report.summary.ok){ exit 1 }
exit 0
