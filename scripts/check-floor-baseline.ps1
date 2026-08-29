$ErrorActionPreference = 'Stop'
$baselinePath = Join-Path $PSScriptRoot '..\proof\floor\baseline.json'
if (-not (Test-Path $baselinePath)) {
    Write-Error "FLOOR baseline file not found at $baselinePath"
    exit 1
}
$baseline = Get-Content $baselinePath -Raw | ConvertFrom-Json
$minimumFloor = [int]$baseline.minimum_floor

Set-Location (Join-Path $PSScriptRoot '..')
$results = cargo test --workspace --locked 2>&1 | Select-String 'test result:'
if ($null -eq $results) {
    Write-Error "FLOOR: cargo test produced no 'test result:' lines"
    exit 1
}
$totalPassed = 0
foreach ($line in $results) {
    if ($line.Line -match '(\d+)\s+passed') {
        $totalPassed += [int]$Matches[1]
    }
}
if ($totalPassed -lt $minimumFloor) {
    Write-Error "FLOOR violation: $totalPassed tests < baseline $minimumFloor"
    exit 1
}
Write-Output "FLOOR OK: $totalPassed tests >= baseline $minimumFloor"
exit 0