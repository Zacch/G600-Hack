# Installs the G600 switcher and editor for the current user and makes the
# switcher start with Windows.
#
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1             # install / update
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Uninstall  # remove
#
# Copies target\release\g600-switcher.exe and g600-editor.exe to
# %LOCALAPPDATA%\Programs\G600, puts a shortcut in your Startup folder, and
# (re)starts the switcher from there. Your profiles in %APPDATA%\g600cfg are
# not touched. Best run from a normal PowerShell window: programs started from
# inside a packaged app (such as the Claude desktop app) get AppData
# redirected, so the script then installs but doesn't start the switcher.
# Build first with:  cargo build --release -p g600-app

param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'

$dest     = Join-Path $env:LOCALAPPDATA 'Programs\G600'
$startup  = [Environment]::GetFolderPath('Startup')
$shortcut = Join-Path $startup 'G600 switcher.lnk'
$source   = Join-Path $PSScriptRoot '..\target\release'
$exes     = 'g600-switcher.exe', 'g600-editor.exe'

function Test-Redirected([string]$path) {
    # Under a packaged app, AppData folders resolve into ...\AppData\Local\Packages\...
    $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    if (-not $item) { return $false }
    $target = @($item.Target) -join ''
    return ($target -match '\\AppData\\Local\\Packages\\')
}

function Stop-Switcher {
    $running = Get-Process g600-switcher -ErrorAction SilentlyContinue
    if ($running) {
        Write-Host 'Stopping the running switcher...'
        $running | Stop-Process -Force
        Start-Sleep -Milliseconds 500
    }
}

if ($Uninstall) {
    Stop-Switcher
    if (Test-Path -LiteralPath $shortcut) { Remove-Item -LiteralPath $shortcut; Write-Host "Removed $shortcut" }
    if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse; Write-Host "Removed $dest" }
    Write-Host 'Uninstalled. Your profiles in %APPDATA%\g600cfg were kept.'
    return
}

foreach ($exe in $exes) {
    if (-not (Test-Path -LiteralPath (Join-Path $source $exe))) {
        throw "$exe not found in $source. Build first: cargo build --release -p g600-app"
    }
}
if (Get-Process g600-editor -ErrorAction SilentlyContinue) {
    throw 'The G600 profile editor is open. Close it (save your changes first), then run this again.'
}

$created = -not (Test-Path -LiteralPath $dest)
New-Item -ItemType Directory -Force -Path $dest | Out-Null
if (Test-Redirected $dest) {
    # The install folder would only exist inside another app's package folder,
    # invisible to Windows and the Startup shortcut.
    if ($created) { Remove-Item -LiteralPath $dest -Recurse -Force }
    throw 'The install folder is redirected into another app''s package folder (this PowerShell was probably started from the Claude app). Run this script from a normal PowerShell or Windows Terminal window instead.'
}

Stop-Switcher
foreach ($exe in $exes) {
    Copy-Item -LiteralPath (Join-Path $source $exe) -Destination $dest -Force
}
Write-Host "Copied $($exes -join ', ') to $dest"

$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut($shortcut)
$lnk.TargetPath = Join-Path $dest 'g600-switcher.exe'
$lnk.WorkingDirectory = $dest
$lnk.Description = 'G600 switcher: per-app mouse profiles'
$lnk.Save()
Write-Host "Created startup shortcut $shortcut"

# A switcher started from a redirected environment would read the wrong
# profiles.toml (the copy in the other app's package folder), so only start it
# when %APPDATA%\g600cfg is the real one.
$cfg = Join-Path $env:APPDATA 'g600cfg'
if ((Test-Redirected $cfg) -or (Test-Redirected (Join-Path $cfg 'profiles.toml'))) {
    Write-Host 'Not starting the switcher from here: this PowerShell has AppData redirected (started from another app).'
    Write-Host "Start it yourself by double-clicking the shortcut in $startup, or sign out and in again."
} else {
    Start-Process -FilePath (Join-Path $dest 'g600-switcher.exe') -WorkingDirectory $dest
    Write-Host 'Started the switcher (see the tray).'
}
Write-Host 'The switcher will now start automatically when you log in.'
