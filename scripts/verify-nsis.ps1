# Exercise an actual NSIS package on a disposable GitHub Actions account.
param(
    [Parameter(Mandatory=$true)][string]$SetupPattern,
    [Parameter(Mandatory=$true)][string]$Id,
    [Parameter(Mandatory=$true)][string]$Name,
    [Parameter(Mandatory=$true)][string]$CliName
)
$ErrorActionPreference = 'Stop'
if (-not $env:RUNNER_TEMP -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'This verification runs only on a disposable GitHub Actions account.'
}
$root = Join-Path $env:RUNNER_TEMP ($Id + '-' + [guid]::NewGuid().ToString('N'))
$env:ARCADE_HOME = Join-Path $root 'arcade'
$receiptPath = Join-Path $env:ARCADE_HOME ('installs\' + $Id + '.json')
$installDir = Join-Path $env:LOCALAPPDATA ('Programs\' + $Name)
$setups = @(Get-ChildItem -Path $SetupPattern -File)
$receipt = $null

function Run([string]$Exe, [string]$Arguments) {
    $process = Start-Process -FilePath $Exe -ArgumentList $Arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        $log = Join-Path $installDir 'arcade-integration-error.log'
        if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Raw | Write-Host }
        throw "$Exe exited $($process.ExitCode)."
    }
}
function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
try {
    Assert ($setups.Count -eq 1) 'Expected exactly one NSIS setup executable.'
    Assert (-not (Test-Path -LiteralPath $installDir)) 'An app is already installed on this runner.'
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    # Both options are explicit so silent parity is independent of defaults.
    Run $setups[0].FullName '/S /STARTATLOGIN=1 /DESKTOPSHORTCUT=0'
    $receipt = Get-Content -LiteralPath $receiptPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert ($receipt.schema -eq 1 -and $receipt.id -eq $Id) 'Invalid install receipt.'
    Assert ($receipt.method -eq 'windows-installer' -and $receipt.managedBy -eq 'self') 'Wrong install method or manager.'
    Assert ((Split-Path -Parent $receipt.path) -ieq $installDir) 'Wrong per-user install location.'
    Assert (Test-Path -LiteralPath $receipt.path) 'Missing installed executable.'
    Assert (Test-Path -LiteralPath $receipt.integration.desktopEntry) 'Missing Start menu entry.'
    Assert (Test-Path -LiteralPath $receipt.integration.autostart) 'Missing selected login entry.'
    Assert (-not $receipt.integration.desktopShortcut) 'Unchecked desktop task wrote an entry.'
    Assert (Test-Path -LiteralPath $receipt.integration.cli) 'Missing CLI shim.'
    Assert ((Split-Path -Leaf $receipt.integration.cli) -eq ($CliName + '.cmd')) 'Wrong CLI shim name.'
    Assert ((Get-Content -LiteralPath $receipt.integration.cli -Raw) -match '%\*') 'CLI shim does not forward arguments.'
    $metadataPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $Id
    $metadata = Get-ItemProperty -LiteralPath $metadataPath
    Assert ($metadata.Publisher -eq 'qa-p1' -and $metadata.DisplayVersion -eq $receipt.version) 'Wrong Apps and features metadata.'
    $sentinel = Join-Path $root 'user-data\keep.txt'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $sentinel) | Out-Null
    Set-Content -LiteralPath $sentinel -Value 'preserve'
    # _?= prevents NSIS from returning before a forked uninstaller finishes.
    Run $receipt.integration.uninstaller ('/S _?=' + $installDir)
    foreach ($entry in @($receiptPath, $receipt.integration.cli, $receipt.integration.desktopEntry, $receipt.integration.autostart)) {
        Assert (-not (Test-Path -LiteralPath $entry)) ('Uninstall retained ' + $entry)
    }
    Assert (-not (Test-Path -LiteralPath $metadataPath)) 'Uninstall retained Apps and features metadata.'
    Assert (Test-Path -LiteralPath $sentinel) 'Uninstall removed unrelated data.'
    Write-Host 'NSIS silent install/uninstall passed: receipt, per-user layout, CLI, task parity, Start menu and cleanup.'
} finally {
    if ($receipt -and (Test-Path -LiteralPath $receiptPath)) {
        try { Run $receipt.integration.uninstaller ('/S _?=' + $installDir) }
        catch { Write-Warning "Installer cleanup failed: $_" }
    }
    if ($receipt -and (Test-Path -LiteralPath $installDir)) {
        Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
