param(
  [Parameter(Mandatory=$true)]
  [string]$InstallerPath
)
$ErrorActionPreference = "Stop"
$expected = @{
  "FileOrbit_0.5.0_x64-setup.exe" = "6c27e0f25382f6ceb673f3c4775fad87e992e2c71d304081682e12800e368e5f"
  "FileOrbit_0.5.0_x64_en-US.msi" = "43a9d0d6e125f91333dd88087412720a37588c6ad1b7cf8aa3823e8f460ec780"
}
$item = Get-Item -LiteralPath $InstallerPath
if ($item.PSIsContainer) { throw "InstallerPath must be a file." }
if (-not $expected.ContainsKey($item.Name)) { throw "Unexpected installer filename: $($item.Name). Use the frozen V0.5 RC2 MSI or NSIS installer." }
$actual = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$wanted = $expected[$item.Name]
if ($actual -ne $wanted) { throw "SHA-256 mismatch for $($item.Name). Expected $wanted but got $actual. Refusing this installer." }
Write-Host "PASS: frozen V0.5 RC2 installer identity verified."
Write-Host "File: $($item.FullName)"
Write-Host "SHA-256: $actual"
