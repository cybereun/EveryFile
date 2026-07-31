[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'

$releaseBuild = Join-Path $repoRoot 'src-tauri\target\release\build'
$crypto = @(
    Get-ChildItem -LiteralPath $releaseBuild -Filter libcrypto.lib -File -Recurse `
        -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*openssl-build\install\lib\libcrypto.lib' } |
        Sort-Object LastWriteTimeUtc -Descending
) | Select-Object -First 1
if ($null -ne $crypto) {
    $installRoot = $crypto.Directory.Parent.FullName
    if ((Test-Path -LiteralPath (Join-Path $installRoot 'lib\libssl.lib')) -and
        (Test-Path -LiteralPath (Join-Path $installRoot 'include\openssl\ssl.h'))) {
        $env:OPENSSL_DIR = $installRoot
        $env:OPENSSL_STATIC = '1'
        $env:OPENSSL_NO_VENDOR = '1'
    }
}

Push-Location -LiteralPath $repoRoot
try {
    & npx tauri build --debug --features e2e --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "E2E desktop build failed." }
} finally {
    Pop-Location
}
