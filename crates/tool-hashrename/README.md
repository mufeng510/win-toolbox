# tool-hashrename

从 `HashRename` 项目移植的核心业务逻辑：三重内容去重 + 自然排序两阶段不覆盖重命名。

## 移植内容

保留（13 个 core 模块 + 26 个集成测试）：
- 只扫当前目录、不递归子目录
- 大小预筛 → 哈希（md5 / sha256 / xxh3）→ 逐字节二次验证
- dry-run 预览模式（零修改、不取目录锁、不恢复历史任务）
- 自然排序、`max(3, 位数)` 序号规则、扩展名保留
- 两阶段重命名 + journal + 崩溃恢复 + 目录锁防并发
- 重复文件走系统回收站（`trash` crate），失败安全停止

丢弃：
- 原 Tauri 外壳、CLI、GUI 前端
- 原 platform 模块中的右键菜单注册、控制台附加、消息框
- 原 CI / 安装包 / 版本号脚本 / 多平台菜单适配

## 对外接口

- `HashRenameTool` — 实现 `toolbox_core::Tool`，`id = "hash-rename"`。从 `ctx.options` 读取 `hash` / `dry_run` / `verbose`。
- `run_with_progress(target, options, on_progress)` — 带中文进度回调；核心在工作线程运行，进度经 channel 回传调用线程，因此 `on_progress` 不必是 `'static`。

## 编译验证

```bash
cargo check -p tool-hashrename --all-targets
cargo test -p tool-hashrename
```
