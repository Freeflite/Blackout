# Installs Blackout for the current user: builds it, copies it to %LOCALAPPDATA%\Programs\Blackout,
# starts it at sign-in, adds a Start Menu shortcut and starts it in the tray. Re-run it to upgrade.
#
#   .\install.ps1                   build and install
#   .\install.ps1 -DesktopShortcut  also put a shortcut on the desktop
#   .\install.ps1 -Uninstall        remove it (settings in %APPDATA%\Blackout are kept)
param([switch]$DesktopShortcut, [switch]$Uninstall, [switch]$NoBuild)
$ErrorActionPreference = 'Stop'

$dest   = Join-Path $env:LOCALAPPDATA 'Programs\Blackout'
$exe    = Join-Path $dest 'blackout.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$startMenuLink = Join-Path ([Environment]::GetFolderPath('Programs')) 'Blackout.lnk'
$desktopLink   = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Blackout.lnk'

function Stop-Blackout {
    # Windows won't replace a running .exe. Ask the copy in the tray to wake and quit (so brightness is
    # put back), and close any open settings window, which saves as it closes.
    if (Test-Path $exe) { Start-Process $exe -ArgumentList '--exit' -Wait }
    foreach ($process in @(Get-Process blackout -ErrorAction SilentlyContinue | Where-Object Path -eq $exe)) {
        if (-not $process.HasExited) { [void]$process.CloseMainWindow() }
        if (-not $process.WaitForExit(15000)) { $process.Kill() }
    }
}

if ($Uninstall) {
    Stop-Blackout
    Remove-ItemProperty $runKey -Name Blackout -ErrorAction SilentlyContinue
    foreach ($link in $startMenuLink, $desktopLink) { if (Test-Path $link) { Remove-Item $link } }
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    "Blackout removed. Settings are still in $env:APPDATA\Blackout."
    return
}

if (-not $NoBuild) {
    cargo build --release --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
    if ($LASTEXITCODE) { throw 'cargo build failed' }
}
Stop-Blackout
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'target\release\blackout.exe') $exe -Force

Set-ItemProperty $runKey -Name Blackout -Value "`"$exe`" --resident"

$links = @($startMenuLink)
if ($DesktopShortcut) { $links += $desktopLink }
$shell = New-Object -ComObject WScript.Shell
foreach ($link in $links) {
    if (Test-Path $link) { Remove-Item $link }   # start clean; an old shortcut's arguments would survive an edit
    $shortcut = $shell.CreateShortcut($link)
    $shortcut.TargetPath  = $exe
    $shortcut.Description = 'Black out monitors now, or toggle the copy in the tray'
    $shortcut.Save()
}

Start-Process $exe -ArgumentList '--resident'
"Installed $exe and started it in the tray."
