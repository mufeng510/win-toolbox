<#
  命令行批量转换（供脚本/自动化调用）

  用法:
    .\批量转换-命令行.ps1 "C:\素材\某目录"                    # 质量 95，转换后删除原 HEIC
    .\批量转换-命令行.ps1 "C:\素材\某目录" -质量 90 -保留原文件
    .\批量转换-命令行.ps1 "C:\素材\某目录" -不递归
#>
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$目录,

  [ValidateRange(1, 100)]
  [int]$质量 = 95,

  [switch]$保留原文件,

  [switch]$不递归
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$exe = Join-Path $PSScriptRoot 'HeicToJpg.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "找不到 $exe" }
if (-not (Test-Path -LiteralPath $目录)) { throw "目录不存在: $目录" }

$递归 = if ($不递归) { '0' } else { '1' }
$删除 = if ($保留原文件) { '0' } else { '1' }

# GUI 程序不会让 PowerShell 自动等待，用 Start-Process 显式等待并取回日志
$logFile = [System.IO.Path]::GetTempFileName()
$proc = Start-Process -FilePath $exe `
        -ArgumentList @('--selftest', "`"$目录`"", $质量, $递归, $删除) `
        -Wait -PassThru -RedirectStandardOutput $logFile -NoNewWindow

$code = $proc.ExitCode

if (Test-Path $logFile) {
    Get-Content -LiteralPath $logFile -Encoding UTF8 | Where-Object { $_ -ne '' } | ForEach-Object { Write-Host $_ }
    Remove-Item -LiteralPath $logFile -Force -ErrorAction SilentlyContinue
}

switch ($code) {
    0 { Write-Host "完成。" -ForegroundColor Green }
    1 { Write-Host "完成，但有文件失败，请查看上方日志。" -ForegroundColor Red }
    default { Write-Host "执行失败（退出码 $code），请确认已安装 ImageMagick。" -ForegroundColor Red }
}

exit $code