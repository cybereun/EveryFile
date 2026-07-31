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
$executableDirectory = [System.IO.Path]::GetDirectoryName($ExecutablePath)
$parserPath = Join-Path $executableDirectory 'everyfile-parser.exe'
$ocrPath = Join-Path $executableDirectory 'everyfile-ocr.exe'

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

foreach ($path in @($ExecutablePath, $parserPath, $ocrPath)) {
    $subsystem = Get-PeSubsystem $path
    if ($subsystem -ne 2) {
        throw "$path uses PE subsystem $subsystem instead of Windows GUI subsystem 2."
    }
}

if (-not ('EveryFile.NativeIcon' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace EveryFile {
  public static class NativeIcon {
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern uint ExtractIconEx(
      string file, int index, IntPtr[] large, IntPtr[] small, uint icons);
    [DllImport("user32.dll")]
    public static extern bool DestroyIcon(IntPtr icon);
  }
}
'@
}
$largeIcon = [IntPtr[]]::new(1)
$smallIcon = [IntPtr[]]::new(1)
$iconCount = [EveryFile.NativeIcon]::ExtractIconEx(
    $ExecutablePath, 0, $largeIcon, $smallIcon, 1
)
if ($iconCount -lt 1 -or ($largeIcon[0] -eq [IntPtr]::Zero -and
        $smallIcon[0] -eq [IntPtr]::Zero)) {
    throw "EveryFile.exe does not contain an extractable application icon."
}
foreach ($icon in @($largeIcon[0], $smallIcon[0])) {
    if ($icon -ne [IntPtr]::Zero) {
        [void][EveryFile.NativeIcon]::DestroyIcon($icon)
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

Write-Output 'PASS: EveryFile, parser, and OCR use the Windows GUI subsystem; the app icon/title are correct; no child console process appeared.'
