# Dot-source before local builds. Existing tool binaries are used in place;
# all new task caches, temp files and build output stay on L:.
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([IO.Path]::GetPathRoot($taskRoot) -ne 'L:\') {
    throw 'This local build profile requires the project on L:.'
}
$taskCache = Join-Path $taskRoot '.cache'
foreach ($part in @('temp', 'npm', 'cargo', 'local-app-data')) {
    [IO.Directory]::CreateDirectory((Join-Path $taskCache $part)) | Out-Null
}
$env:TEMP = Join-Path $taskCache 'temp'
$env:TMP = $env:TEMP
$env:npm_config_cache = Join-Path $taskCache 'npm'
$env:CARGO_HOME = Join-Path $taskCache 'cargo'
$env:CARGO_TARGET_DIR = Join-Path $taskRoot 'src-tauri\target'
$env:CARGO_BUILD_JOBS = '2'
$env:LOCALAPPDATA = Join-Path $taskCache 'local-app-data'

# Keep Cargo's registry/index inside the task cache. An older cache profile
# may contain a source replacement pointing at a user's C: registry; replace
# that generated config instead of reading or mutating the C: drive.
$cargoConfig = @"
[net]
offline = false
"@
[IO.File]::WriteAllText(
    (Join-Path $env:CARGO_HOME 'config.toml'),
    $cargoConfig,
    [Text.UTF8Encoding]::new($false)
)

if (Test-Path -LiteralPath 'C:\Strawberry\perl\bin\perl.exe') {
    $env:PATH = 'C:\Strawberry\perl\bin;' + $env:PATH
}
