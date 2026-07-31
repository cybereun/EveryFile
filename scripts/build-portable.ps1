[CmdletBinding()]
param(
    [string]$Configuration = 'release'
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$package = Get-Content -Raw (Join-Path $repoRoot 'package.json') | ConvertFrom-Json
$version = [string]$package.version
$targetRoot = Join-Path $repoRoot "src-tauri\target\$Configuration"
$appExe = Join-Path $targetRoot 'EveryFile.exe'
$sidecarSource = Join-Path $repoRoot 'src-tauri\binaries\everyfile-parser-x86_64-pc-windows-msvc.exe'
$notice = Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md'
$readme = Join-Path $repoRoot 'README.md'
$releaseRoot = Join-Path $repoRoot 'artifacts\release'
$portableRoot = Join-Path $repoRoot 'artifacts\portable'
$stageRoot = Join-Path $portableRoot 'EveryFile'
$zipPath = Join-Path $releaseRoot "EveryFile-Portable-v$version.zip"

$required = @($appExe, $sidecarSource, $notice, $readme)
foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required portable resource is missing: $path"
    }
}

foreach ($path in @($releaseRoot, $portableRoot)) {
    [System.IO.Directory]::CreateDirectory($path) | Out-Null
}

$resolvedPortableRoot = [System.IO.Path]::GetFullPath($portableRoot)
$resolvedStageRoot = [System.IO.Path]::GetFullPath($stageRoot)
if (-not $resolvedStageRoot.StartsWith(
        $resolvedPortableRoot + [System.IO.Path]::DirectorySeparatorChar,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
    throw "Refusing to replace an unsafe staging path: $resolvedStageRoot"
}
if (Test-Path -LiteralPath $resolvedStageRoot) {
    Remove-Item -LiteralPath $resolvedStageRoot -Recurse -Force
}
[System.IO.Directory]::CreateDirectory($resolvedStageRoot) | Out-Null

Copy-Item -LiteralPath $appExe -Destination (Join-Path $resolvedStageRoot 'EveryFile.exe')
Copy-Item -LiteralPath $sidecarSource -Destination (Join-Path $resolvedStageRoot 'everyfile-parser.exe')
Copy-Item -LiteralPath $notice -Destination $resolvedStageRoot
Copy-Item -LiteralPath $readme -Destination $resolvedStageRoot

if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}
Compress-Archive -LiteralPath $resolvedStageRoot -DestinationPath $zipPath -CompressionLevel Optimal

$installerCandidates = @(
    Get-ChildItem -LiteralPath (Join-Path $targetRoot 'bundle\nsis') -Filter '*-setup.exe' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending
)
if ($installerCandidates.Count -eq 0) {
    throw "NSIS installer was not found under $targetRoot\bundle\nsis"
}
$installerPath = Join-Path $releaseRoot "EveryFile-Setup-v$version.exe"
Copy-Item -LiteralPath $installerCandidates[0].FullName -Destination $installerPath -Force

$checksums = @($installerPath, $zipPath) | ForEach-Object {
    $hash = Get-FileHash -LiteralPath $_ -Algorithm SHA256
    "$($hash.Hash.ToLowerInvariant())  $([System.IO.Path]::GetFileName($_))"
}
$checksumPath = Join-Path $releaseRoot 'SHA256SUMS.txt'
[System.IO.File]::WriteAllLines($checksumPath, $checksums, [System.Text.UTF8Encoding]::new($false))

[ordered]@{
    installer = $installerPath
    portable = $zipPath
    checksums = $checksumPath
} | ConvertTo-Json
