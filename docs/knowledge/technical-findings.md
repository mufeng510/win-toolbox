# 技术核实记录

## GitHub Actions Windows runner 与本机环境差异

- **来源**：GitHub Actions 官方 runner 文档 + 本机实际环境排查
- **日期**：2026-11-22
- **适用范围**：CI 构建与本地构建的环境配置
- **结论**：
  - `windows-latest` runner 自带 Rust、MSVC 工具链、Windows SDK 与 WebView2 Runtime，Tauri v2 的 `npm run tauri build` 可直接运行
  - workflow 用 `ilammy/msvc-dev-cmd@v1` 显式注入 MSVC 的 PATH/LIB/INCLUDE，避免环境差异
  - 本机 `scripts/cargo.ps1` 硬编码了 VS 2022 BuildTools 14.44 与 SDK 19041 的绝对路径，**仅适用于当前机器**，CI 不调用它

## CI 发版实测通过（v0.2.0）

- **来源**：实际推送 tag `v0.2.0` 后的 GitHub Actions 运行结果（run `45038088968`）
- **日期**：2026-11-22
- **适用范围**：本项目 Windows 发版
- **结论**：
  - workflow 全绿（8m9s），Release `win-toolbox v0.2.0` 自动创建，作者为 `github-actions[bot]`
  - 产物路径是工作区根的 `target/release/bundle/`（不是 `src-tauri/target/`）——Cargo workspace 的 target 在根目录
  - 发布资产：`win-toolbox_0.2.0_x64-setup.exe`（约 2.9 MiB）与 `win-toolbox_0.2.0_x64_en-US.msi`（约 4.4 MiB）；下载后文件头分别为 `4D-5A`（PE）与 `D0-CF-11-E0`（OLE 复合文档），均有效
  - 首次运行因产物路径写成 `src-tauri/target/...` 导致 Show artifacts 失败、发布被跳过；已修正
  - 运行中有 Node.js 20 弃用警告（GitHub 强制相关 action 在 Node 24 上运行），不影响构建与发布

## Windows 11 经典菜单双 CLSID 方案已真机验证

- **来源**：用户在 Windows 11 真机实测确认（2026-11-22）
- **适用范围**：本工具的 classic-menu 开关
- **结论**：双 CLSID 方案可正常在经典菜单与新菜单之间切换，用户确认验证通过、无问题
- **注意**：该机制来自社区报道、非微软官方支持；Windows 大版本更新后需复测

## Windows 右键多选文件传参不可靠

- **来源**：微软官方文档 `How to Employ the Verb Selection Model` + 社区实测（2015/2019 年多个报告互相矛盾）
- **日期**：2026-11-22
- **适用范围**：需要「选中多个文件 → 右键 → 一次拿到全部路径」的工具
- **结论**：官方只定义 `MultiSelectModel`（Single/Player/Document）语义，未承诺命令行一次收到所有路径；`%*` + Player 对非打包 exe 行为不一致。因此 `image-merge` 采用 GUI 多选入口，不做右键多选。
