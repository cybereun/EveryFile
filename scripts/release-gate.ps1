[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Join-Path $repoRoot 'src-tauri\Cargo.toml'
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'

Push-Location -LiteralPath $repoRoot
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
    & npm test -- --run
    if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed.' }
    & cargo fmt --manifest-path $manifest -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    & cargo clippy --manifest-path $manifest --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust Clippy check failed.' }
    & powershell -ExecutionPolicy Bypass -File scripts/test-rust.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }

    foreach ($testTarget in @('ocr_privacy', 'ai_secrets', 'ai_providers', 'folder_discovery')) {
        & cargo test --manifest-path $manifest --test $testTarget
        if ($LASTEXITCODE -ne 0) {
            throw "Privacy gate failed: $testTarget"
        }
    }

    $trackedSources = @(
        Get-ChildItem -LiteralPath (Join-Path $repoRoot 'src') -Recurse -File
        Get-ChildItem -LiteralPath (Join-Path $repoRoot 'src-tauri\src') -Recurse -File
    )
    $placeholderPattern = '(?i)(api[_-]?key\s*[:=]\s*["''][^"'']+|sk-[A-Za-z0-9]{16,}|AIza[A-Za-z0-9_-]{20,})'
    foreach ($source in $trackedSources) {
        if ((Get-Content -Raw -LiteralPath $source.FullName) -match $placeholderPattern) {
            throw "Possible cloud secret placeholder found in $($source.FullName)"
        }
    }
    Write-Output 'PASS: frontend, Rust, privacy, and secret-placeholder release gates passed.'
} finally {
    Pop-Location
}
