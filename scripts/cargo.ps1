# 在 Windows 上以正确的 MSVC/Windows SDK 环境运行 cargo。
# 用法: powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 check --workspace
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$ErrorActionPreference = 'Stop'

$env:RUSTUP_TOOLCHAIN = 'stable'

$sdk = 'C:\Program Files (x86)\Windows Kits\10'
$sdkVersion = '10.0.19041.0'
$msvcRoot = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC'
$msvcDir = Get-ChildItem $msvcRoot -Directory -ErrorAction Stop |
    Sort-Object Name -Descending |
    Select-Object -First 1
if (-not $msvcDir) { throw 'MSVC toolchain directory not found' }
$msvc = $msvcDir.FullName

$env:PATH = "$msvc\bin\Hostx64\x64;$env:PATH"
$env:LIB = "$sdk\Lib\$sdkVersion\um\x64;$sdk\Lib\$sdkVersion\ucrt\x64;$msvc\lib\x64"
$env:INCLUDE = "$sdk\Include\$sdkVersion\um;$sdk\Include\$sdkVersion\ucrt;$msvc\include"

& "$env:USERPROFILE\.cargo\bin\cargo.exe" @CargoArgs
exit $LASTEXITCODE
