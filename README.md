# win-toolbox

个人 Windows 工具箱：把多个小工具集合进**一个 Tauri v2 应用**，统一维护、统一安装、统一注册右键菜单。

## 设计原则

- **单 exe 多工具**：无参数启动显示工具列表；右键菜单调用 `win-toolbox.exe --tool <id> --path "%V"`，进入同一个 exe 的分流逻辑。
- **单一事实来源**：每个工具在自己的 `Tool` trait 实现中声明元数据与右键菜单能力，外壳从注册表自动生成工具列表和菜单；新增工具只需实现 trait 并注册一次，不会出现清单与代码漂移。
- **整体发版**：不做插件架构；新增工具 = 加一个 crate + 在清单和注册表里登记。
- **跨平台内核 + 平台适配**：核心逻辑用 Rust 写成独立 crate，Windows 为主，macOS/Linux 先保证核心功能可用。
- **系统设置隔离**：修改系统状态的工具有 `system_tweak` 标记，不在安装时默认启用，必须在 GUI 显式确认。

## 当前工具

| id | 名称 | 右键菜单 | 说明 |
| --- | --- | --- | --- |
| `hash-rename` | Hash 去重并重命名 | 文件夹 / 文件夹空白处 | 三重内容去重（大小+哈希+逐字节），重复文件进回收站，剩余自然排序重命名为 001、002……只处理当前文件夹 |
| `heic-to-jpg` | 批量 HEIC 转 JPG | 文件夹空白处 | 纯 Rust 解码 HEIC/HEIF 转 JPG，支持质量、递归、EXIF 方向、删原文件前校验 |
| `classic-menu` | 恢复 Windows 11 经典右键菜单 | 无（系统设置） | 双 CLSID 方案，HKCU、免管理员、可一键还原 |
| `folder-flatten` | 文件夹平铺 | 文件夹 / 文件夹空白处 | 把直接子文件夹里的文件移到本文件夹，重名自动加 `(1)`、`(2)` 绝不覆盖；移动后删除空子文件夹，只处理一层 |
| `image-merge` | 合并切片图片 | 无（GUI 多选） | 多选被切成上下两半的图片，按文件名排序两两合并；宽度必须一致，输出到第一张图所在文件夹并删除原图 |

## 目录结构

```text
win-toolbox/
├── Cargo.toml                 # Rust workspace
├── package.json / vite.config.ts / tsconfig.json
├── frontend/                  # 原生 TS + Vite 界面
├── crates/
│   ├── toolbox-core/          # Tool trait / 配置 / 错误
│   ├── tool-hashrename/       # 从 HashRename 移植的核心 + 测试
│   ├── tool-heic2jpg/         # Rust 版 HEIC 转换
│   ├── tool-classic-menu/     # 经典菜单开关（Windows only）
│   ├── tool-flatten/          # 文件夹平铺
│   └── tool-image-merge/      # 切片图片合并
└── src-tauri/                 # Tauri 外壳：命令、分流、菜单管理器
```

## 开发环境

- Rust（stable，建议 ≥ 1.89，见 `crates/tool-heic2jpg` 的 rust-version）
- Node.js ≥ 18 + npm
- Windows：MSVC Build Tools（Tauri 打包需要 `link.exe`）
- Tauri CLI 由 `npm run tauri` 提供（无需全局安装）

```bash
npm install
npm run tauri dev          # 开发模式（热重载）
npm run build              # 前端类型检查 + 构建
```

Rust 部分在 Windows 上需要 MSVC 链接器与 Windows SDK 的库路径。本项目提供脚本自动设置环境变量：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 check --workspace
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 test --workspace
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 build --release
```

打包：先设置上述环境变量（或用 VS 开发人员命令提示符），再执行 `npm run tauri build`（NSIS，当前用户安装）。

### 真实 HEIC 文件验证

`tool-heic2jpg` 带一个开发示例，可直接对某个目录里的真实 HEIC 跑转换（默认质量 85、不递归、不删原文件）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 run -p tool-heic2jpg --example convert_dir -- <目录>
```

验证记录：使用 `dsoprea/heic-exif-samples` 的 4 个带 EXIF 样本（41KB–3MB），4/4 转换成功，输出 JPG 用 System.Drawing 打开均有效（如 3992×2992），源文件保留；重复运行正确跳过同名 JPG。

### 新增工具验证记录

- `folder-flatten`：临时目录含 2 个有文件的子目录（其中一个与主目录有同名 `a.txt`）、1 个空目录、1 个带嵌套目录的子目录 → 3 个文件移动成功、`a.txt` 自动变为 `a (1).txt` 且内容正确、3 个空目录删除、嵌套目录及其文件原样保留。
- `image-merge`：把一张 700×476 的 JPG 切成两张 700×238 的切片 → 输出 `01-top_合并.jpg`（700×476、JPEG），输出到第一张图所在文件夹，两张原图按预期删除。

无界面调用加 `--quiet` / `-q` 可不弹消息框、只把结果写到终端，便于脚本与测试；GUI 和右键菜单入口不受影响：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 run --bin win-toolbox -- --tool folder-flatten --path <目录> --quiet
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 run --bin win-toolbox -- --tool image-merge --path <图1> --path <图2> --quiet
```

## 右键菜单

GUI「右键菜单管理」区可逐个勾选安装/卸载，也可命令行：

```bat
win-toolbox.exe --install-menus
win-toolbox.exe --uninstall-menus
```

注册位置均在 HKCU（当前用户，免管理员）。命令模板：

```text
"<exe>" --tool <id> --path "%V"
```

## Windows 11 经典菜单开关（安全说明）

- **做什么**：在 `HKCU\Software\Classes\CLSID` 下写入两个 CLSID 的空 `InprocServer32` 值，让 Explorer 回退到经典菜单。
- **为什么**：2026-11 的 KB5132061 之后，旧的单 CLSID 方案已失效，需同时写入两个。
- **影响**：只对当前用户生效；需重启资源管理器；不提权、不碰系统文件。
- **可逆性**：点「恢复新菜单」即删除这两个键；覆盖前的值备份在 `%APPDATA%\win-toolbox\classic-menu-backup.json`。
- **注意**：该机制未被微软官方文档支持，Windows 更新可能再次改变行为；代码里已注明来源为社区实测报道。

## 迁移说明

- HashRename 只移植了 core（算法、journal、目录锁、回收站）与原测试；原 Tauri 壳、CLI、GUI、CI、安装包、其他平台的菜单适配均已移除。
- HeicToJpg 的 C# 实现与 PowerShell 安装脚本被 Rust crate 取代；ImageMagick 外部依赖已去掉。
- HEVC/HEIF 涉及第三方专利，`imazen/heic` 仅授予版权许可，个人使用风险低，商用请自行评估。

## License

本仓库自身为 MIT（`LICENSE`）。注意**依赖许可差异**：

- `heic` 0.1.6（`imazen/heic`）为 **AGPL-3.0-only OR Imazen-Commercial** 双许可，不是 MIT。个人本地使用不受影响；若把打包后的程序分发给他人且不想公开源码，需评估 AGPL 的网络使用条款或取得 Imazen 商业授权，详见 `crates/tool-heic2jpg/README.md`。
- HEVC/HEIF 还涉及第三方专利，商用分发需另行评估。
