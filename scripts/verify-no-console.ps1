[CmdletBinding()]
param(
    [string]$ExecutablePath
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
    $ExecutablePath = Join-Path $repoRoot 'artifacts\portable\EveryFile\EveryFile.exe'
}
$ExecutablePath = [System.IO.Path]::GetFullPath($ExecutablePath)
$sidecarPath = Join-Path ([System.IO.Path]::GetDirectoryName($ExecutablePath)) 'everyfile-parser.exe'

function Get-PeSubsystem([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Executable is missing: $Path"
    }
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "Not a PE executable: $Path"
        }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "Invalid PE signature: $Path"
        }
        $stream.Position = $peOffset + 24
        $magic = $reader.ReadUInt16()
        if ($magic -ne 0x10B -and $magic -ne 0x20B) {
            throw "Unsupported PE optional header: $Path"
        }
        $stream.Position = $peOffset + 24 + 68
        return $reader.ReadUInt16()
    } finally {
        $stream.Dispose()
    }
}

foreach ($path in @($ExecutablePath, $sidecarPath)) {
    $subsystem = Get-PeSubsystem $path
    if ($subsystem -ne 2) {
        throw "$path uses PE subsystem $subsystem instead of Windows GUI subsystem 2."
    }
}

$process = Start-Process -FilePath $ExecutablePath `
    -WorkingDirectory ([System.IO.Path]::GetDirectoryName($ExecutablePath)) `
    -PassThru -WindowStyle Hidden
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) {
            throw "EveryFile exited before its main window appeared (exit code $($process.ExitCode))."
        }
    } until ($process.MainWindowHandle -ne 0 -or [DateTime]::UtcNow -ge $deadline)

    if ($process.MainWindowHandle -eq 0) {
        throw 'EveryFile did not create a visible main window within 30 seconds.'
    }
    if ($process.MainWindowTitle -ne 'EveryFile') {
        throw "Unexpected main window title: '$($process.MainWindowTitle)'"
    }

    $consoleProcesses = @(
        Get-CimInstance Win32_Process |
            Where-Object {
                $_.ParentProcessId -eq $process.Id -and
                $_.Name -in @('conhost.exe', 'cmd.exe', 'powershell.exe', 'pwsh.exe')
            }
    )
    if ($consoleProcesses.Count -gt 0) {
        throw "A console process was created by EveryFile: $($consoleProcesses.Name -join ', ')"
    }
} finally {
    if (-not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(10000)) {
            Stop-Process -Id $process.Id -Force
            $process.WaitForExit()
        }
    }
}

Write-Output 'PASS: EveryFile and its parser use the Windows GUI subsystem, the app title is correct, and no child console process appeared.'
