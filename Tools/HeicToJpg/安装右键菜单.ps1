# 安装右键菜单：在任意文件夹的空白处右键显示“批量 HEIC 转 JPG”
# 写入 HKCU（当前用户），不需要管理员权限，也不影响其他用户
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$ErrorActionPreference = 'Stop'
$exe    = Join-Path $PSScriptRoot 'HeicToJpg.exe'
$menuId = 'HeicToJpg_Convert'
$key    = "HKCU:\Software\Classes\Directory\Background\shell\$menuId"

if (-not (Test-Path -LiteralPath $exe)) { throw "找不到 $exe" }

New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name '(default)' -Value '批量 HEIC 转 JPG'
Set-ItemProperty -Path $key -Name 'MUIVerb'      -Value '批量 HEIC 转 JPG'

# 图标取自 ImageMagick
$magick = Get-ChildItem 'C:\Program Files\ImageMagick*\magick.exe','C:\Tools\ImageMagick*\magick.exe' -ErrorAction SilentlyContinue |
          Select-Object -First 1 -ExpandProperty FullName
if ($magick) { Set-ItemProperty -Path $key -Name 'Icon' -Value "$magick,0" }

New-Item -Path "$key\command" -Force | Out-Null
Set-ItemProperty -Path "$key\command" -Name '(default)' -Value "`"$exe`" `"%V`""

Write-Host "右键菜单已安装。" -ForegroundColor Green
Write-Host "  位置: 文件夹空白处右键"
Write-Host "  命令: `"$exe`" `"%V`""
Write-Host ""
Write-Host "如果菜单没立刻出现，重启资源管理器或注销后重登即可生效。" -ForegroundColor DarkGray