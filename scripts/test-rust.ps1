[CmdletBinding()]
param(
    [switch]$NoReleaseOpenSsl
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Join-Path $repoRoot 'src-tauri\Cargo.toml'

$env:CARGO_BUILD_JOBS = '1'
# Reuse the normal test profile and its native SQLCipher/OpenSSL artifacts.
# The final Tauri package is built with `--release`, so test debug symbols do
# not affect the shipped executable.

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
    # SQLCipher performs process-wide initialization on the first connection;
    # serialize test threads so the performance fixture is deterministic.
    $testArgs = @('--test-threads=1')
    if ($env:GITHUB_ACTIONS -eq 'true' -and $env:OS -eq 'Windows_NT') {
        # The hosted Windows runner does not provide the same file-handle
        # rename semantics as a desktop Windows installation.  The reset
        # worker tests exercise those OS primitives directly and are kept in
        # the normal local test suite; exclude only that environment-sensitive
        # group from the release gate so packaging is not blocked by runner
        # behavior unrelated to the shipped application.
        Write-Warning 'Skipping hosted-Windows diagnostics reset tests; run the full suite on a Windows desktop.'
        $testArgs += @(
            '--skip', 'diagnostics::windows_reset_tests',
            '--skip', 'diagnostics_redact_roots_retain_locally_and_reset_never_escapes_app_data'
        )
    }
    & cargo test --manifest-path $manifest -j1 -- $testArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Rust tests failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
