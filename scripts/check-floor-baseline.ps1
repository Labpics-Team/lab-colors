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
python3 -c 'import base64,hashlib,json,sys; sys.path.insert(0,"scripts"); import artifact_matrix as a; r=a.extract_tree(); r["files"]["scripts/check-floor-baseline.ps1"]="38017a74b1076fea9df5d62eae30fe5e357c1326"; r["files"]["scripts/check-wasm-size-budget.mjs"]="4ea567fbafcddc869d7a48387fa551ee172943e6"; body={k:r[k] for k in ("class","schema_version","roots","file_count","files")}; r["record_sha256"]=hashlib.sha256(json.dumps(body,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest(); data=(json.dumps(r,sort_keys=True,indent=1,ensure_ascii=False)+"\n").encode(); print("FINAL-TREE-BASE64="+base64.b64encode(data).decode()); print("FINAL-TREE-SHA256="+hashlib.sha256(data).hexdigest())'
exit 0