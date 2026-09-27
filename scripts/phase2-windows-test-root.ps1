param(
  [ValidateSet('Prepare','Verify','Cleanup')][string]$Mode = 'Prepare',
  [string]$Root
)
$ErrorActionPreference = 'Stop'
$temp = [IO.Path]::GetFullPath($env:TEMP)
if ($Mode -eq 'Prepare') {
  $Root = Join-Path $temp ('FileOrbit-Test-Phase2-' + [guid]::NewGuid().ToString('N'))
} elseif (-not $Root) { throw 'Verify/Cleanup requires -Root from Prepare output.' }
$full = [IO.Path]::GetFullPath($Root)
$relative = [IO.Path]::GetRelativePath($temp, $full)
if ($relative -eq '.' -or [IO.Path]::IsPathRooted($relative) -or $relative -eq '..' -or $relative.StartsWith('..' + [IO.Path]::DirectorySeparatorChar)) {
  throw 'Only a disposable Test Root under TEMP is allowed.'
}
if (-not ([IO.Path]::GetFileName($full) -match '^FileOrbit-Test-Phase2-[a-f0-9]{32}$')) { throw 'Invalid fixture folder name.' }
$fixture = Join-Path $full 'testdata\fixtures'
if ($Mode -eq 'Prepare') {
  foreach ($folder in @('과제24년~\D-Project\회의록','과제24년~\D-Project\견적','과제24년~\D-Project\설계','과제24년~\연구과제A','개인업무\보고서','개인업무\참고자료','기타')) {
    [void](New-Item -ItemType Directory -Path (Join-Path $fixture $folder) -Force)
  }
  $names = @('과제24년~\D-Project\회의록\회의 (1).TXT','과제24년~\D-Project\견적\quote v2.pdf','과제24년~\D-Project\설계\PFD_설계.dwg','개인업무\보고서\월간 보고서.docx')
  foreach ($name in $names) { [IO.File]::WriteAllText((Join-Path $fixture $name), 'Synthetic FileOrbit Phase 2 fixture') }
  [IO.File]::WriteAllText((Join-Path $full '.fileorbit-phase2-fixture'), 'synthetic-only')
  Write-Output "TestRoot=$full"
  Write-Output "ScanRoot=$fixture"
  Write-Output 'Only synthetic files were created. Enter both paths in the FileOrbit Test Root panel.'
} else {
  if (-not (Test-Path (Join-Path $full '.fileorbit-phase2-fixture'))) { throw 'Fixture marker missing; refusing action.' }
  if ((Get-Item -LiteralPath $full).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse point rejected.' }
  if ($Mode -eq 'Verify') {
  if (-not (Test-Path (Join-Path $full 'data\fileorbit.db'))) { throw 'SQLite DB missing under Test Root.' }
  Write-Output 'Test Root DB exists. Verify indexed counts and restart persistence in the app UI.'
  } else {
    Remove-Item -LiteralPath $full -Recurse -Force
    Write-Output 'Disposable Test Root removed.'
  }
}
