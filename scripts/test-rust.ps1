[CmdletBinding()]
param(
    [switch]$NoReleaseOpenSsl
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Join-Path $repoRoot 'src-tauri\Cargo.toml'

$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'

if (-not $NoReleaseOpenSsl) {
    $releaseBuild = Join-Path $repoRoot 'src-tauri\target\release\build'
    $crypto = @(
        Get-ChildItem -LiteralPath $releaseBuild `
            -Filter libcrypto.lib -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -like '*openssl-build\install\lib\libcrypto.lib' } |
            Sort-Object LastWriteTimeUtc -Descending
    ) | Select-Object -First 1
    if ($null -ne $crypto) {
        $installRoot = $crypto.Directory.Parent.FullName
        $ssl = Join-Path $installRoot 'lib\libssl.lib'
        $headers = Join-Path $installRoot 'include\openssl\ssl.h'
        if ((Test-Path -LiteralPath $ssl) -and (Test-Path -LiteralPath $headers)) {
            $env:OPENSSL_DIR = $installRoot
            $env:OPENSSL_STATIC = '1'
            $env:OPENSSL_NO_VENDOR = '1'
            Write-Output "Reusing release OpenSSL: $installRoot"
        }
    }
}

Push-Location -LiteralPath $repoRoot
try {
    & cargo test --manifest-path $manifest -j1
    if ($LASTEXITCODE -ne 0) {
        throw "Rust tests failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
