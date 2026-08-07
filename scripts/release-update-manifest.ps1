[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $repoRoot
try {
    & npm run generate:update-manifest
    if ($LASTEXITCODE -ne 0) { throw 'Updater manifest generation failed.' }
} finally {
    Pop-Location
}
