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
    & node scripts/build-parser-sidecar.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Parser sidecar build failed.' }

    & npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }

    & powershell -ExecutionPolicy Bypass -File scripts/build-portable.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Portable packaging failed.' }

    & powershell -ExecutionPolicy Bypass -File scripts/verify-no-console.ps1
    if ($LASTEXITCODE -ne 0) { throw 'No-console verification failed.' }
} finally {
    Pop-Location
}
