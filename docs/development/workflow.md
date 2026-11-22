# 开发与发版流程

## 本地开发

```powershell
npm install
npm run tauri dev          # GUI 冒烟测试，需交互式桌面会话
npm run build              # 前端类型检查 + 构建
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 test --workspace
```

`scripts/cargo.ps1` 自动设置本机 MSVC + Windows SDK 的 PATH/LIB/INCLUDE；换机器需按 AGENTS.md 的「已知待验证」重装环境。

## 发版

1. 改代码 → 跑测试 → 提交
2. 打标签并推送：`git tag v0.2.0` → `git push origin v0.2.0`
3. GitHub Actions 自动：从 tag 同步版本 → `npm ci` → `npm run tauri build` → 上传 NSIS/MSI 到 Release
4. 也可在 Actions 页面手动触发 `Release` workflow，输入版本号；留空则保留代码中的当前版本

workflow 定义在 `.github/workflows/release.yml`，仅申请 `contents: write` 权限，使用内置 `GITHUB_TOKEN`，无外部凭证。

## 安装包说明

- NSIS 采用「当前用户安装」（不写 HKLM、不提权），与右键菜单只写 HKCU 的约束一致
- 未签名，首次安装会触发 Windows「未知发布者」提示；个人使用可点「仍要运行」
