# 技术核实记录

## GitHub Actions Windows runner 与本机环境差异

- **来源**：GitHub Actions 官方 runner 文档 + 本机实际环境排查
- **日期**：2026-11-22
- **适用范围**：CI 构建与本地构建的环境配置
- **结论**：
  - `windows-latest` runner 自带 Rust、MSVC 工具链、Windows SDK 与 WebView2 Runtime，Tauri v2 的 `npm run tauri build` 可直接运行
  - workflow 用 `ilammy/msvc-dev-cmd@v1` 显式注入 MSVC 的 PATH/LIB/INCLUDE，避免环境差异
  - 本机 `scripts/cargo.ps1` 硬编码了 VS 2022 BuildTools 14.44 与 SDK 19041 的绝对路径，**仅适用于当前机器**，CI 不调用它

## Windows 右键多选文件传参不可靠

- **来源**：微软官方文档 `How to Employ the Verb Selection Model` + 社区实测（2015/2019 年多个报告互相矛盾）
- **日期**：2026-11-22
- **适用范围**：需要「选中多个文件 → 右键 → 一次拿到全部路径」的工具
- **结论**：官方只定义 `MultiSelectModel`（Single/Player/Document）语义，未承诺命令行一次收到所有路径；`%*` + Player 对非打包 exe 行为不一致。因此 `image-merge` 采用 GUI 多选入口，不做右键多选。
