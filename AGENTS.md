# AGENTS.md · win-toolbox

## 项目定位
个人 Windows 工具箱，单 Tauri v2 应用集合多个小工具。Windows 为主，核心逻辑尽量跨平台。整体发版，不做插件架构。

## 硬性约束
1. **不要修改 `Tools/HeicToJpg/`**（旧 C# 工具，仅作功能参考，保留原样直到迁移验证完成）。
2. `tools/manifest.json` 是右键菜单与工具列表的唯一事实来源；新增/删除工具必须同步它，避免幽灵菜单。
3. 右键菜单只写 HKCU（当前用户），不提权；菜单命令统一为 `"<exe>" --tool <id> --path "%V"`。
4. 系统设置类工具（`system_tweak: true`）不得在安装时默认启用，GUI 操作前必须有明确的中文确认弹窗。
5. 前端配置键名与 Rust crate 内部键名不同，统一在 `src-tauri/src/commands.rs::normalize_options` 做映射：
   - `algorithm` → `hash`，`preview` → `dry_run`
   - `delete_original` → `delete_source`
6. 每个工具 crate 必须实现 `toolbox_core::Tool` trait，并通过 `src-tauri/src/lib.rs::build_registry` 注册。

## 文件边界
- `crates/toolbox-core`：共享接口，改动会影响所有工具，需谨慎。
- `crates/tool-*`：各工具内部实现，互不依赖。
- `src-tauri/src/{commands,menu,dispatch}.rs`：外壳层，负责配置映射、注册表、参数分流。

## 已知待验证
- `heic` 0.1.6 为 **AGPL-3.0-only OR Imazen-Commercial**（非 MIT）；个人本地使用可行，分发前必须重新评估许可，详见 `crates/tool-heic2jpg/README.md`。
- 本机已安装 Rust 1.100.0、VS Build Tools（MSVC 14.44）+ Windows 10 SDK 19041，并通过 rsproxy 镜像加速依赖下载；这些是环境前置条件，换机器时需重装。
- 经典菜单的双 CLSID 方案来自社区报道，未在真机验证；修改后必须实测。
- GUI 冒烟测试需在**交互式桌面会话**中进行；Agent 的执行环境是 Session 0，`tauri dev` 会因 WebView2 无法创建窗口而失败（ERROR_INVALID_WINDOW_HANDLE），属环境限制。
- 编译/测试一律用 `scripts\cargo.ps1`（自动设置 MSVC + Windows SDK 的 PATH/LIB/INCLUDE），不要直接调用 cargo。

## 沟通与交付
- 用户使用中文，所有 UI 文案、注释、文档用中文。
- 系统级修改先说明「做什么 / 为什么 / 影响 / 可逆性 / 备份 / 回滚」再执行。
