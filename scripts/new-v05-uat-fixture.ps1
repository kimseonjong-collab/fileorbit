param(
  [string]$Root = (Join-Path $env:TEMP "FileOrbit-UAT")
)

$ErrorActionPreference = "Stop"
$rootFull = [System.IO.Path]::GetFullPath($Root)
$tempFull = [System.IO.Path]::GetFullPath($env:TEMP)

if (-not $rootFull.StartsWith($tempFull, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to create fixture outside TEMP: $rootFull"
}
if ([System.IO.Path]::GetFileName($rootFull.TrimEnd('\')) -ne "FileOrbit-UAT") {
  throw "Fixture directory must be named FileOrbit-UAT."
}
if (Test-Path $rootFull) {
  throw "Fixture already exists. Refusing to overwrite: $rootFull"
}

$downloads = Join-Path $rootFull "Downloads"
$work = Join-Path $rootFull "Work"
$projects = Join-Path $work "Projects"
$reference = Join-Path $work "Reference"

New-Item -ItemType Directory -Path $downloads,$projects,$reference -Force | Out-Null

@{
  "project-note.txt" = "Synthetic FileOrbit UAT project note."
  "meeting-note.txt" = "Synthetic FileOrbit UAT meeting note."
  "reference-note.txt" = "Synthetic FileOrbit UAT reference note."
} | ForEach-Object {
  foreach ($name in $_.Keys) {
    Set-Content -LiteralPath (Join-Path $downloads $name) -Value $_[$name] -Encoding utf8NoBOM
  }
}

$manifest = [ordered]@{
  fixture = "FileOrbit-UAT"
  synthetic_only = $true
  created_utc = (Get-Date).ToUniversalTime().ToString("o")
  root = $rootFull
  downloads = $downloads
  work = $work
  files = @("project-note.txt","meeting-note.txt","reference-note.txt")
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $rootFull "fixture.json") -Encoding utf8NoBOM

Write-Host "Created disposable FileOrbit UAT fixture:"
Write-Host "  Root:      $rootFull"
Write-Host "  Downloads: $downloads"
Write-Host "  Work:      $work"
Write-Host "No real user files were read, moved, renamed, or deleted."
