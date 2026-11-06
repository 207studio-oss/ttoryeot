# 또렷 성능 기준선: 저장소 안의 합성 문서만으로 쪽/초와 최대 메모리를 잰다.
# 사용법: pwsh -File tools\bench.ps1 [-Runs 100] [-Warmup 5]
param(
    [ValidateRange(1, 10000)]
    [int]$Runs = 100,
    [ValidateRange(0, 1000)]
    [int]$Warmup = 5
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$document = Join-Path $root 'hwp-core\tests\fixtures\synthetic.hwpx'
$fonts = Join-Path $root 'assets\fonts'

if (-not (Test-Path -LiteralPath $document -PathType Leaf)) {
    throw "fixture not found: $document"
}
if (-not (Test-Path -LiteralPath $fonts -PathType Container)) {
    throw "font directory not found: $fonts"
}

# 실제 측정은 채점기의 bench 명령이 한다. 여기서는 저장소 안의 합성 문서만 넘긴다.
Push-Location $root
try {
    & cargo run --release --locked -p ttoryeot-score -- bench `
        --document $document `
        --fonts $fonts `
        --runs $Runs `
        --warmup $Warmup
    if ($LASTEXITCODE -ne 0) {
        throw 'benchmark failed'
    }
} finally {
    Pop-Location
}
