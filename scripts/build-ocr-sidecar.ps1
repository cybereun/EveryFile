[CmdletBinding()]
param(
    [switch]$SkipDependencyInstall,
    [switch]$SkipExecutableBuild
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$hostRoot = Join-Path $repoRoot 'sidecar\ocr-host'
$modelsRoot = Join-Path $hostRoot 'models'
$cacheRoot = Join-Path $repoRoot '.cache\ocr-build'
$downloadsRoot = Join-Path $cacheRoot 'downloads'
$venvRoot = Join-Path $cacheRoot 'venv'
$manifestPath = Join-Path $hostRoot 'model-manifest.json'
$distRoot = Join-Path $cacheRoot 'dist'
$targetName = 'everyfile-ocr-x86_64-pc-windows-msvc.exe'
$targetPath = Join-Path $repoRoot "src-tauri\binaries\$targetName"

$models = @(
    [ordered]@{
        name = 'PP-OCRv5_mobile_det'
        version = 'Paddle3.0.0'
        license = 'Apache-2.0'
        url = 'https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_det_infer.tar'
    },
    [ordered]@{
        name = 'korean_PP-OCRv5_mobile_rec'
        version = 'Paddle3.0.0'
        license = 'Apache-2.0'
        url = 'https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0//korean_PP-OCRv5_mobile_rec_infer.tar'
    },
    [ordered]@{
        name = 'PP-DocLayout-S'
        version = 'Paddle3.0.0'
        license = 'Apache-2.0'
        url = 'https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-DocLayout-S_infer.tar'
    },
    [ordered]@{
        name = 'PP-FormulaNet-S'
        version = 'Paddle3.0.0'
        license = 'Apache-2.0'
        url = 'https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-FormulaNet-S_infer.tar'
    }
)

foreach ($directory in @($modelsRoot, $downloadsRoot, $distRoot)) {
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
}

foreach ($model in $models) {
    $archive = Join-Path $downloadsRoot "$($model.name).tar"
    $destination = Join-Path $modelsRoot $model.name
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        Invoke-WebRequest -Uri $model.url -OutFile $archive -UseBasicParsing
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) {
        & tar.exe -xf $archive -C $modelsRoot
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to extract OCR model: $($model.name)"
        }
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) {
        $archiveRoot = Join-Path $modelsRoot "$($model.name)_infer"
        if (Test-Path -LiteralPath $archiveRoot -PathType Container) {
            Move-Item -LiteralPath $archiveRoot -Destination $destination
        }
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) {
        throw "OCR archive did not create the expected model directory: $destination"
    }
    $modelFiles = @(Get-ChildItem -LiteralPath $destination -File -Recurse)
    if ($modelFiles.Count -eq 0) {
        throw "OCR model directory is empty: $destination"
    }
}

$manifestFiles = foreach ($file in Get-ChildItem -LiteralPath $modelsRoot -File -Recurse |
        Sort-Object FullName) {
    $relative = $file.FullName.Substring($modelsRoot.Length + 1).Replace('\', '/')
    [ordered]@{
        path = $relative
        size = $file.Length
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
$manifest = [ordered]@{
    schemaVersion = 1
    generatedFrom = @($models | ForEach-Object {
        $archive = Join-Path $downloadsRoot "$($_.name).tar"
        [ordered]@{
            name = $_.name
            version = $_.version
            license = $_.license
            source = $_.url
            archiveSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    })
    files = @($manifestFiles)
}
[System.IO.File]::WriteAllText(
    $manifestPath,
    ($manifest | ConvertTo-Json -Depth 8),
    [System.Text.UTF8Encoding]::new($false)
)

if (-not $SkipExecutableBuild) {
    $uv = Get-Command uv -ErrorAction Stop
    if (-not (Test-Path -LiteralPath (Join-Path $venvRoot 'Scripts\python.exe'))) {
        & $uv.Source venv --python 3.12 $venvRoot
        if ($LASTEXITCODE -ne 0) {
            throw 'Failed to create the Python 3.12 OCR build environment.'
        }
    }
    $python = Join-Path $venvRoot 'Scripts\python.exe'
    if (-not $SkipDependencyInstall) {
        $env:UV_CACHE_DIR = Join-Path $cacheRoot 'uv'
        & $uv.Source pip install --python $python --requirement (Join-Path $hostRoot 'requirements.lock')
        if ($LASTEXITCODE -ne 0) {
            throw 'Failed to install the pinned OCR build dependencies.'
        }
    }
    & $python -m PyInstaller `
        --noconfirm `
        --clean `
        --distpath $distRoot `
        --workpath (Join-Path $cacheRoot 'work') `
        (Join-Path $hostRoot 'everyfile-ocr.spec')
    if ($LASTEXITCODE -ne 0) {
        throw 'PyInstaller failed to build the OCR sidecar.'
    }
    $built = Join-Path $distRoot 'everyfile-ocr.exe'
    if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
        throw "OCR sidecar output is missing: $built"
    }
    Copy-Item -LiteralPath $built -Destination $targetPath -Force

    $stream = [System.IO.File]::OpenRead($targetPath)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        $stream.Position = 0x3c
        $peOffset = $reader.ReadInt32()
        $stream.Position = $peOffset + 4 + 20 + 68
        $subsystem = $reader.ReadUInt16()
    } finally {
        if ($null -ne $reader) { $reader.Dispose() }
        $stream.Dispose()
    }
    if ($subsystem -ne 2) {
        throw "OCR sidecar is not a Windows GUI-subsystem executable (value: $subsystem)."
    }
}

[ordered]@{
    models = $models.Count
    manifest = $manifestPath
    executable = if ($SkipExecutableBuild) { $null } else { $targetPath }
} | ConvertTo-Json
