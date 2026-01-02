#Requires -Version 5.1
$ErrorActionPreference="Stop"
function Write-Utf8NoBom {
  param([string]$Path,[string]$Content)
  $fullPath = if ([System.IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path -Path (Get-Location) -ChildPath $Path }
  $enc=New-Object System.Text.UTF8Encoding($false)
  $dir=Split-Path -Parent $fullPath
  if($dir -and -not(Test-Path $dir)){New-Item -ItemType Directory -Force -Path $dir|Out-Null}
  [System.IO.File]::WriteAllText($fullPath,$Content,$enc)
}
Write-Utf8NoBom .\src\config.rs (Get-Content -Raw .\scripts\_config.rs.txt)
Write-Utf8NoBom .\src\audit.rs (Get-Content -Raw .\scripts\_audit.rs.txt)
Write-Utf8NoBom .\src\dlp.rs (Get-Content -Raw .\scripts\_dlp.rs.txt)
Write-Utf8NoBom .\src\opa.rs (Get-Content -Raw .\scripts\_opa.rs.txt)
Write-Utf8NoBom .\src\approvals.rs (Get-Content -Raw .\scripts\_approvals.rs.txt)
Write-Utf8NoBom .\src\gateway.rs (Get-Content -Raw .\scripts\_gateway.rs.txt)
Write-Utf8NoBom .\src\bundle.rs (Get-Content -Raw .\scripts\_bundle.rs.txt)
Write-Utf8NoBom .\src\tools\mod.rs (Get-Content -Raw .\scripts\_tools_mod.rs.txt)
Write-Utf8NoBom .\src\tools\registry.rs (Get-Content -Raw .\scripts\_tools_registry.rs.txt)
Write-Utf8NoBom .\src\tools\sandbox\mod.rs (Get-Content -Raw .\scripts\_sandbox_mod.rs.txt)
Write-Utf8NoBom .\src\tools\sandbox\native.rs (Get-Content -Raw .\scripts\_sandbox_native.rs.txt)
Write-Host "Wrote remaining Rust files."