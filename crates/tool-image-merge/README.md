# 图片上下合并（tool-image-merge）

把多张图片按文件名顺序两两配对并**垂直拼接**成一张，输出保存在第一张图片所在文件夹。实现 `toolbox_core::Tool`，工具 id 为 `image-merge`。

## 功能与规则

- **输入**：`RunContext.options.paths` 是图片路径的字符串数组（GUI 多选传入）；`ctx.target` 可为 `None`。`paths` 为空或缺省时返回参数错误。
- **排序**：按**文件名自然排序**后再配对，连续数字按数值比较，例如 `img2` 排在 `img10` 前。比较的是文件名，不是所在目录。
- **配对**：排序后第 1+2、3+4……为一对；奇数张时最后一张跳过，并在报告的 `details` 中列出。
- **宽度校验**：每一对的两张图片宽度必须**完全相同**，否则该对报错并跳过，但**继续处理其它对**。最终 `ToolReport.ok` 当且仅当没有任何一对失败。
- **拼接**：画布宽 `w = 第一张宽`，高 `h = h1 + h2 - overlap`。第一张像素放 `0..h1`；第二张裁掉顶部 `overlap` 行后放到 `h1..h1+h2-overlap`。统一用 `Rgba8` 画布，透明通道不会丢失。
- **输出格式（跟随原图）**：两张都是 JPEG → 输出 `jpg`（使用 `quality`）；否则输出 `png`。
- **输出位置与命名**：写到**第一张图片所在文件夹**，名称 = 第一张去掉扩展名 + `_合并` + 对应扩展名。若同名文件已存在，依次追加 ` (1)`、` (2)`……**绝不覆盖已有文件**。

## 配置键

| 键 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `paths` | string[] | 必填 | 待合并的图片路径，至少两张才能形成配对 |
| `delete_sources` | bool | `true` | 成功后是否删除原图（不可逆，默认删除） |
| `quality` | u8 | `95` | 仅 JPG 输出使用，范围 1–100 |
| `overlap` | usize | `0` | 第二张顶部被裁掉的重叠像素行数，必须小于第二张高度 |

## 删除原图的安全流程（不可逆操作）

`delete_sources = true` 时，每一对都严格执行以下顺序：

1. 解码两张原图，校验宽度一致并在内存中完成拼接；
2. 在同目录写入**隐藏临时文件** `.imgmerge-<pid>-<时间戳>-<序号>.tmp`（`create_new` 独占创建，避免撞名）；
3. 重新打开临时文件，**验证能解码且尺寸与内存画布一致**；
4. 用 `rename` 把临时文件改成最终文件名；
5. **只有以上全部成功**，才删除这一对的两张原图。

任何一步失败都会清理临时文件、跳过该对，并**保留该对全部原图**；失败原因写入报告的 `details`。报告同时列出每个成功输出文件和实际被删除的原图。

> ⚠️ 删除原图不可恢复。GUI 侧在调用前应有明确的中文确认提示；本 crate 只负责按上述安全顺序执行。

## 编译与验证

从仓库根目录执行（不要直接调用 `cargo`，脚本会注入 MSVC / Windows SDK 环境）：

```powershell
# 只检查本 crate
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 check -p tool-image-merge

# 运行本 crate 的单元测试（含临时目录完整流程）
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 test -p tool-image-merge

# 加入工作区整体检查
powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 check --workspace
```

> 说明：新增 crate 还需主 Agent 把 `crates/tool-image-merge` 加入根 `Cargo.toml` 的 `workspace.members`/`workspace.dependencies`，并在 `src-tauri/src/lib.rs::build_registry` 注册；本 crate 的写入范围仅限自身目录，未改动这些文件。
