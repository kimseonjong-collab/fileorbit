param(
  [string]$Root = (Join-Path $env:TEMP "FileOrbit-UAT")
)

$ErrorActionPreference = "Stop"
$rootFull = [System.IO.Path]::GetFullPath($Root)
$tempFull = [System.IO.Path]::GetFullPath($env:TEMP)
$manifestPath = Join-Path $rootFull "fixture.json"

if (-not $rootFull.StartsWith($tempFull, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to remove anything outside TEMP: $rootFull"
}
if ([System.IO.Path]::GetFileName($rootFull.TrimEnd('\')) -ne "FileOrbit-UAT") {
  throw "Refusing to remove directory not named FileOrbit-UAT."
}
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Synthetic fixture manifest is missing; refusing cleanup."
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.fixture -ne "FileOrbit-UAT" -or $manifest.synthetic_only -ne $true) {
  throw "Fixture manifest identity check failed; refusing cleanup."
}
if ([System.IO.Path]::GetFullPath([string]$manifest.root) -ne $rootFull) {
  throw "Fixture manifest root mismatch; refusing cleanup."
}

Remove-Item -LiteralPath $rootFull -Recurse -Force
Write-Host "Removed disposable FileOrbit UAT fixture only: $rootFull"
