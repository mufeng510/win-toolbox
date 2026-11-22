# 移除右键菜单（只删注册表项，不删程序文件）
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$menuId = 'HeicToJpg_Convert'
$key    = "HKCU:\Software\Classes\Directory\Background\shell\$menuId"

if (Test-Path $key) {
    Remove-Item -Path $key -Recurse -Force
    Write-Host "右键菜单已移除。" -ForegroundColor Green
} else {
    Write-Host "右键菜单本来就不存在。" -ForegroundColor DarkGray
}

Write-Host "如需彻底移除，请手动删除 C:\Tools\HeicToJjpg 目录。" -ForegroundColor DarkGray