param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

# Keep ASCII-only to avoid PowerShell 5.1 encoding issues on Windows runners.
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Invalid version format: '$Version'. Expected e.g. 0.2.0 or 0.2.0-beta.1"
}

$root = Split-Path -Parent $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

# All crates use version.workspace = true, so only the workspace version needs updating.
$cargoPath = Join-Path $root 'Cargo.toml'
$cargoRaw = Get-Content -Path $cargoPath -Raw
$cargoPattern = '(\[workspace\.package\][\s\S]*?version\s*=\s*")([^"]+)(")'
if (-not [regex]::IsMatch($cargoRaw, $cargoPattern)) {
    throw 'Cannot locate [workspace.package] version in Cargo.toml'
}
$cargoRaw = [regex]::Replace($cargoRaw, $cargoPattern, '${1}' + $Version + '${3}')
[System.IO.File]::WriteAllText($cargoPath, $cargoRaw, $utf8NoBom)

# Tauri uses this version for the bundle filename and app metadata.
$tauriPath = Join-Path $root 'src-tauri/tauri.conf.json'
$tauriRaw = Get-Content -Path $tauriPath -Raw
$tauriPattern = '("version"\s*:\s*")([^"]+)(")'
if (-not [regex]::IsMatch($tauriRaw, $tauriPattern)) {
    throw 'Cannot locate version in src-tauri/tauri.conf.json'
}
$tauriRaw = [regex]::Replace($tauriRaw, $tauriPattern, '${1}' + $Version + '${3}')
[System.IO.File]::WriteAllText($tauriPath, $tauriRaw, $utf8NoBom)

Write-Host "Version synchronized to $Version"
