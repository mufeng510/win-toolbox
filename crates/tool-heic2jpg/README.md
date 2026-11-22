# tool-heic2jpg

批量把文件夹内的 HEIC / HEIF 转成 JPG，**纯 Rust 实现**，不再需要 ImageMagick / `magick.exe`。

实现 `toolbox-core::Tool` trait，`id = "heic-to-jpg"`，`platforms = ["windows", "macos", "linux"]`；元数据与右键菜单能力由 trait 声明，外壳从注册表自动生成，无需额外清单。

## 功能

- 转换目标文件夹内的 `.heic` / `.heif`（扩展名大小写不敏感），输出为同目录同名的 `.jpg`，不建子目录
- 按 EXIF Orientation 校正方向；HEIF 容器自身的 `irot` / `imir` 变换由 heic 解码器处理
- 输出 8-bit RGB（sRGB 数值），JPEG 不携带 ICC / EXIF（等效原版 ImageMagick 的 `-strip`）
- JPG 质量 1–100 可调，默认 95
- 可选递归子文件夹，默认开启
- 可选转换成功后删除原 HEIC，默认开启；**删除前会重新读取磁盘上的 JPG 并完整解码校验**，失败则保留原文件并删除损坏输出
- 已有同名非空 JPG 则跳过，且不会删除对应 HEIC
- 目录中没有 HEIC 时给出友好提示，不返回错误
- 统计总览 / 成功 / 跳过 / 失败数：有失败项时 `ToolReport.ok = false`（对齐原版退出码 1）

## 输入契约

`RunContext.target` 为要处理的文件夹路径，必填（为 `None` 时返回 `InvalidArgument`）。
`RunContext.options` 为工具配置 JSON，缺省字段使用默认值：

```json
{
  "quality": 95,
  "recursive": true,
  "delete_source": true
}
```

`quality` 会被收敛到 `1..=100`（越界自动夹紧；非数字回落默认值）。

## 依赖说明

| 依赖 | 版本 | 用途 | 说明 |
| --- | --- | --- | --- |
| `heic` | `0.1.6` | HEIC/HEIF 解码 | imazen/heic，HEVC 后端**内置且纯 Rust**，`#![forbid(unsafe_code)]`，无 C/C++、无 vcpkg、无系统库 |
| `image` | `0.25.10` | JPEG 编码 / 方向处理 / JPG 校验 | 仅启用 `jpeg` feature，不拖入 png/gif/rayon 等无关依赖 |

刻意**不使用** `libheif-rs`（它需要系统安装 libheif / vcpkg，会重新引入外部依赖）。

> 关于 feature 名：`heic` 0.1.6（crates.io 当前稳定版）的 feature 为 `std` / `parallel` / `av1` / `unci` 等，**没有** `backend-rust` 这个 feature；`backend-rust` 出现在已 yank 的 0.2.0 README 中。0.1.6 的纯 Rust HEVC 解码器是默认内置的，因此这里使用 `default-features = false, features = ["std"]`。
>
> 如需覆盖 AV1 编码的 HEIF 或未压缩 HEIF，可按需追加 `av1` / `unci` feature（同样是纯 Rust 依赖）；代价是编译时间和安装包体积增大。

**MSRV**：本 crate 声明 `rust-version = "1.89"`（`heic` 0.1.6 要求 1.89，`image` 0.25 要求 1.88）。workspace 默认的 1.82 对本 crate 不适用，请用 ≥ 1.89 的工具链编译。

## 专利声明（重要）

HEVC (H.265) 及其在 HEIF 容器中的使用，可能被第三方专利（包括但不限于 Access Advance HEVC 专利池成员）覆盖。imazen/heic 的作者不持有专利，只授予代码的著作权许可，**不授予任何专利许可**。该解码器仅做解码。

个人使用场景风险较低；若要把本工具随安装包**商业分发**，请自行评估目标司法管辖区的专利风险并咨询专业人士。

## 许可证冲突（需主 Agent / 用户确认）

本 workspace 的 `LICENSE` 是 MIT，而 `heic` 0.1.6 采用 **AGPL-3.0-only OR Imazen-Commercial** 双许可。闭源分发或不想公开源码时，需要考虑购买 Imazen 商业许可，或改用其他解码方案。本 crate 自身沿用 workspace 的 MIT 声明，但**不代表依赖的授权问题已解决**。

## 与原 C# 版的差异

| 项目 | 原 `Tools/HeicToJpg` | 本 crate |
| --- | --- | --- |
| HEIC 解码 | 调用外部 `magick.exe`（ImageMagick） | 内置 heic 纯 Rust 解码，无需安装任何软件 |
| 外观 | WinForms 窗口、DPI 适配、进度条、确认弹窗 | 无 GUI；由统一前端 / CLI 通过 `Tool` trait 调用 |
| 可测试性 | `--probe` / `--selftest` 命令行参数 | 纯函数 + `#[cfg(test)]` 单元测试，`cargo test -p tool-heic2jpg` |
| 输出校验 | `magick identify` | `image` crate 完整解码 JPG + 尺寸检查 |
| EXIF 方向 | ImageMagick `-auto-orient` | heic 提取原始 EXIF，image 的 `Orientation` 应用变换 |
| 色彩 | `-strip` 后为未标记 sRGB | 8-bit RGB（BT.601/709/2020 → RGB），无 ICC/EXIF；PQ/HLG HDR 不做 tone mapping（与原版能力相当） |
| 排序 | 文件名 OrdinalIgnoreCase | 路径字符串大小写不敏感排序 |
| 符号链接 | .NET 会跟随 | 主动跳过，避免目录环与误删链接目标 |
| 质量非法值 | UI 限制 1–100 | JSON 传入后自动夹紧到 1–100 |

行为保持一致的部分：同目录输出、同名 JPG 跳过且不删源、删前校验、有失败项视为整体失败、无 HEIC 时友好提示。

## 测试

```powershell
cargo test -p tool-heic2jpg
```

单元测试覆盖：`collect_targets` 递归 / 非递归与扩展名过滤、`should_skip` 规则、`build_options` 默认值与 quality 边界、`parse_exif_orientation` 大小端 / EXIF 前缀 / 非法输入、`verify_jpeg` 有效与无效字节、同名 JPG 跳过且不删源、无 HEIC 的友好提示、tool 元数据与参数校验。测试使用内存构造的 JPEG 与临时目录，不依赖任何真实 HEIC 样本。

## 已知限制

- 与原版一致：同名 JPG 只要非空就会被当作「已存在」而跳过，即使它本身是损坏文件。
- `av1` / `unci` 未启用，AV1 或未压缩编码的 HEIF 可能无法解码（典型 iPhone HEVC HEIC 不受影响）。
- 不处理 PQ/HLG HDR 的 tone mapping；10-bit HEVC 按 heic 的默认 8-bit 降位输出。
- 不跟随符号链接。
