# Runs the mask-guidance prototype: every candidate x case for several fixed seeds.
# Usage (repo root, runtime installed, release CLI built):
#   ./bench/mask_proto/run.ps1 [-Preset standard] [-Seeds 101,202,303]
param(
    [string]$Preset = "standard",
    [long[]]$Seeds = @(101, 202, 303),
    [string]$Qil = "./target/release/qil.exe"
)
$ErrorActionPreference = "Stop"
$work = Join-Path $PSScriptRoot "work"
python (Join-Path $PSScriptRoot "prepare.py")
foreach ($seed in $Seeds) {
    $out = Join-Path $work "out/seed_$seed"
    & $Qil bench --suite (Join-Path $work "suite.json") --inputs (Join-Path $work "inputs") `
        --out $out --preset $Preset --seed $seed --force
    if ($LASTEXITCODE -ne 0) { throw "qil bench failed for seed $seed" }
}
python (Join-Path $PSScriptRoot "composite.py")
Write-Host "Done. Review: $work/sheets/*.jpg and $work/report.json"
