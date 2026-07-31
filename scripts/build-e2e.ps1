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
    $ocrPython = Join-Path $repoRoot '.cache\ocr-build\venv\Scripts\python.exe'
    $ocrBinary = Join-Path $repoRoot 'src-tauri\binaries\everyfile-ocr-x86_64-pc-windows-msvc.exe'
    if (-not (Test-Path -LiteralPath $ocrPython) -or
        -not (Test-Path -LiteralPath $ocrBinary)) {
        & powershell -ExecutionPolicy Bypass -File scripts/build-ocr-sidecar.ps1
        if ($LASTEXITCODE -ne 0) { throw 'OCR E2E dependency build failed.' }
    }
    & $ocrPython scripts/create-ocr-fixtures.py tests/fixtures/folder-search/ocr-generated
    if ($LASTEXITCODE -ne 0) { throw 'OCR E2E fixture generation failed.' }

    & npx tauri build --debug --features e2e --no-bundle --config src-tauri/tauri.e2e.conf.json
    if ($LASTEXITCODE -ne 0) { throw "E2E desktop build failed." }
} finally {
    Pop-Location
}
