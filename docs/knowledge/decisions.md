# 已确认决策

## 2026-11-22 · GitHub Actions 自动发版

- **来源**：用户在需求确认页明确选择（四项均为推荐项）
- **适用范围**：本项目所有 Release 发布
- **决策**：
  1. 触发方式：push `v*` 标签自动触发 + Actions 页面手动触发（可指定版本号）
  2. 构建平台：仅 Windows（`windows-latest` runner），产物为 NSIS（当前用户安装）与 MSI
  3. 版本来源：从 tag 去掉 `v` 前缀，由 `scripts/set-version.ps1` 同步到根 `Cargo.toml` 和 `src-tauri/tauri.conf.json`
  4. 代码签名：不签名。个人本地使用可行；安装时 Windows 会提示「未知发布者」，已在 README 说明

## 2026-11-22 · 工具元数据单一来源

- **来源**：用户选择「Tool trait 为唯一来源」
- **适用范围**：工具列表与右键菜单注册
- **决策**：删除 `tools/manifest.json`，每个工具在 `Tool` trait 实现中声明元数据与 capabilities，外壳从 `build_registry()` 自动生成列表和菜单；避免清单与代码漂移。
