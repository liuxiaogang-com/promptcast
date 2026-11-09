param([string]$Installer, [int]$AppPid, [string]$Exe, [string]$Job)
$ErrorActionPreference = 'Stop'
$Backup = Join-Path $Job 'previous-app'
$AppDir = Split-Path -LiteralPath $Exe
$Installing = $false
$Restored = $false
try {
    if (!(Test-Path -LiteralPath $Installer -PathType Leaf) -or !(Test-Path -LiteralPath $Exe -PathType Leaf)) {
        throw 'Installer or application is missing.'
    }
    # 在退出之前检查备份目录和目标目录可写；不修改用户数据目录。
    $Probe = Join-Path $AppDir ('.promptcast-update-' + [guid]::NewGuid().ToString())
    [IO.File]::WriteAllText($Probe, '')
    Remove-Item -LiteralPath $Probe
    [IO.File]::WriteAllText((Join-Path $Job 'ready'), 'ready')
    $Running = Get-Process -Id $AppPid -ErrorAction SilentlyContinue
    if ($Running -and !$Running.WaitForExit(30000)) { throw 'Application did not exit; update cancelled.' }
    Copy-Item -LiteralPath $AppDir -Destination $Backup -Recurse
    $Installing = $true
    $Process = Start-Process -FilePath $Installer -ArgumentList '/S' -PassThru -Wait
    if ($Process.ExitCode -ne 0 -and $Process.ExitCode -ne 3010) { throw "Installer failed with exit code $($Process.ExitCode)." }
    if (!(Test-Path -LiteralPath $Exe -PathType Leaf)) { throw 'Installed executable is missing.' }
    Start-Process -FilePath $Exe
    # 保留安装包和备份，便于发生后续启动异常时恢复。
} catch {
    $Message = $_.Exception.Message
    if ($Installing) {
        try {
            Get-ChildItem -LiteralPath $Backup -Force | Copy-Item -Destination $AppDir -Recurse -Force
            $Restored = $true
        } catch { $Message += "`r`nRestore failed: $($_.Exception.Message)" }
    }
    $Message += "`r`nInstaller retained: $Installer`r`nBackup: $Backup"
    $Log = Join-Path $Job 'error.txt'
    [IO.File]::WriteAllText($Log, $Message)
    Start-Process notepad.exe -ArgumentList ('"' + $Log + '"')
    if ((!$Installing -or $Restored) -and !(Get-Process -Id $AppPid -ErrorAction SilentlyContinue)) {
        if (Test-Path -LiteralPath $Exe -PathType Leaf) { Start-Process -FilePath $Exe }
    }
    exit 1
}
