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
$ocrSource = Join-Path $repoRoot 'src-tauri\binaries\everyfile-ocr-x86_64-pc-windows-msvc.exe'
$modelManifest = Join-Path $repoRoot 'sidecar\ocr-host\model-manifest.json'
$modelConfig = Join-Path $repoRoot 'sidecar\ocr-host\models.json'
$ocrRequirements = Join-Path $repoRoot 'sidecar\ocr-host\requirements.lock'
$license = Join-Path $repoRoot 'LICENSE'
$kordocLicense = Join-Path $repoRoot 'vendor\kordoc\LICENSE'
$kordocNotice = Join-Path $repoRoot 'vendor\kordoc\NOTICE'
$rhwpLicense = Join-Path $repoRoot 'node_modules\@rhwp\core\LICENSE'
$notice = Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md'
$readme = Join-Path $repoRoot 'README.md'
$releaseNotes = Join-Path $repoRoot 'RELEASE_NOTES.md'
$releaseRoot = Join-Path $repoRoot 'artifacts\release'
$portableRoot = Join-Path $repoRoot 'artifacts\portable'
$stageRoot = Join-Path $portableRoot 'EveryFile'
$zipPath = Join-Path $releaseRoot "EveryFile-Portable-v$version.zip"

$required = @(
    $appExe, $sidecarSource, $ocrSource, $modelManifest, $modelConfig,
    $ocrRequirements, $license, $kordocLicense, $kordocNotice, $rhwpLicense, $notice, $readme,
    $releaseNotes
)
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
Copy-Item -LiteralPath $ocrSource -Destination (Join-Path $resolvedStageRoot 'everyfile-ocr.exe')
Copy-Item -LiteralPath $notice -Destination $resolvedStageRoot
Copy-Item -LiteralPath $readme -Destination $resolvedStageRoot
Copy-Item -LiteralPath $releaseNotes -Destination $resolvedStageRoot
Copy-Item -LiteralPath $license -Destination $resolvedStageRoot
$licenseRoot = Join-Path $resolvedStageRoot 'licenses'
[System.IO.Directory]::CreateDirectory($licenseRoot) | Out-Null
Copy-Item -LiteralPath $kordocLicense -Destination (Join-Path $licenseRoot 'KORDOC-LICENSE')
Copy-Item -LiteralPath $kordocNotice -Destination (Join-Path $licenseRoot 'KORDOC-NOTICE')
Copy-Item -LiteralPath $rhwpLicense -Destination (Join-Path $licenseRoot 'RHWP-LICENSE')
Copy-Item -LiteralPath $modelManifest -Destination $licenseRoot
Copy-Item -LiteralPath $modelConfig -Destination $licenseRoot
Copy-Item -LiteralPath $ocrRequirements -Destination $licenseRoot

$manifest = Get-Content -Raw -LiteralPath $modelManifest | ConvertFrom-Json
if ($manifest.generatedFrom.Count -ne 4 -or $manifest.files.Count -lt 10) {
    throw 'OCR model manifest is incomplete.'
}
foreach ($model in $manifest.generatedFrom) {
    if ($model.license -ne 'Apache-2.0' -or
        $model.archiveSha256 -notmatch '^[0-9a-f]{64}$' -or
        $model.source -notmatch '^https://') {
        throw "OCR model provenance is incomplete: $($model.name)"
    }
}
$requirements = @(Get-Content -LiteralPath $ocrRequirements |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if ($requirements.Count -eq 0 -or
    @($requirements | Where-Object { $_ -notmatch '^[A-Za-z0-9_.\[\]-]+==[0-9]' }).Count -gt 0) {
    throw 'OCR runtime requirements must be fully pinned.'
}
$noticeText = Get-Content -Raw -LiteralPath $notice
foreach ($requiredNotice in @('Kordoc license', 'rhwp / @rhwp/core license', 'PaddleOCR 3.7.0', 'PaddlePaddle 3.3.1')) {
    if (-not $noticeText.Contains($requiredNotice)) {
        throw "Third-party notice is missing: $requiredNotice"
    }
}

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
Copy-Item -LiteralPath $license -Destination (Join-Path $releaseRoot 'LICENSE') -Force
Copy-Item -LiteralPath $notice -Destination (Join-Path $releaseRoot 'THIRD_PARTY_NOTICES.md') -Force
Copy-Item -LiteralPath $releaseNotes -Destination (Join-Path $releaseRoot 'RELEASE_NOTES.md') -Force

[ordered]@{
    installer = $installerPath
    portable = $zipPath
    checksums = $checksumPath
} | ConvertTo-Json
