[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$strawberryPerlDirectory = 'C:\Strawberry\perl\bin'
$gitPerlDirectory = 'C:\Program Files\Git\usr\bin'
if (Test-Path -LiteralPath (Join-Path $strawberryPerlDirectory 'perl.exe')) {
    $env:PATH = "$strawberryPerlDirectory;$env:PATH"
} elseif ($null -eq (Get-Command perl -ErrorAction SilentlyContinue) -and
    (Test-Path -LiteralPath (Join-Path $gitPerlDirectory 'perl.exe'))) {
    $env:PATH = "$gitPerlDirectory;$env:PATH"
}
if ($null -eq (Get-Command perl -ErrorAction SilentlyContinue)) {
    throw 'A complete Perl runtime is required to build SQLCipher/OpenSSL. Install Strawberry Perl.'
}
Push-Location -LiteralPath $repoRoot
try {
    # Tauri's externalBin entries are validated while Cargo runs clippy. Build
    # the parser sidecar before the release gate so clean GitHub runners have
    # the resource that the Tauri build script expects.
    & node scripts/build-parser-sidecar.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Parser sidecar build failed.' }

    # The Tauri configuration validates both externalBin resources during the
    # release gate. Build OCR before Cargo checks for the same reason as the
    # parser sidecar above.
    & powershell -ExecutionPolicy Bypass -File scripts/build-ocr-sidecar.ps1
    if ($LASTEXITCODE -ne 0) { throw 'OCR sidecar build failed.' }

    & powershell -ExecutionPolicy Bypass -File scripts/release-gate.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Release gate failed.' }

    & npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }

    & powershell -ExecutionPolicy Bypass -File scripts/build-portable.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Portable packaging failed.' }

    & powershell -ExecutionPolicy Bypass -File scripts/verify-no-console.ps1
    if ($LASTEXITCODE -ne 0) { throw 'No-console verification failed.' }

    $env:EVERYFILE_RELEASE_ACCEPTANCE = '1'
    try {
        & npx vitest run tests/e2e/release.spec.ts
        if ($LASTEXITCODE -ne 0) { throw 'Clean release acceptance failed.' }
    } finally {
        Remove-Item Env:EVERYFILE_RELEASE_ACCEPTANCE -ErrorAction SilentlyContinue
    }
} finally {
    Pop-Location
}
