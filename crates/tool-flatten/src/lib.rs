//! 文件夹平铺工具的 toolbox-core 外壳。
//!
//! 对外暴露 [`FlattenTool`](struct.FlattenTool.html)（实现 `Tool` trait）；
//! 核心扫描 / 规划 / 移动逻辑在 [`core`](mod.core.html) 模块中，可单独测试。

pub mod core;

use toolbox_core::{Capabilities, RunContext, Tool, ToolReport, ToolboxError};

use crate::core::{run_flatten, ConflictPolicy, FlattenOptions, FlattenOutcome};

/// 文件夹平铺工具。
pub struct FlattenTool;

impl Tool for FlattenTool {
    fn id(&self) -> &'static str {
        "folder-flatten"
    }

    fn name(&self) -> &'static str {
        "文件夹平铺"
    }

    fn description(&self) -> &'static str {
        "把目标文件夹下每个直接子文件夹里的普通文件移动到目标文件夹根目录，自动处理重名，并删除留下的空子文件夹"
    }

    fn platforms(&self) -> &'static [&'static str] {
        &["windows", "macos", "linux"]
    }

    fn capabilities(&self) -> Capabilities {
        // 右键菜单由外壳层统一声明；本工具不改动系统设置。
        Capabilities::default()
    }

    fn run(&self, ctx: RunContext) -> toolbox_core::Result<ToolReport> {
        let target = ctx
            .target
            .as_deref()
            .ok_or_else(|| ToolboxError::InvalidArgument("缺少目标文件夹".to_string()))?;

        let dry_run = ctx
            .options
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let on_conflict_raw = ctx
            .options
            .get("on_conflict")
            .and_then(|v| v.as_str())
            .unwrap_or("auto_rename");
        let on_conflict = ConflictPolicy::parse(on_conflict_raw).ok_or_else(|| {
            ToolboxError::InvalidArgument(format!(
                "不支持的重名策略: {}（可选: auto_rename / skip）",
                on_conflict_raw
            ))
        })?;

        let options = FlattenOptions { dry_run, on_conflict };
        let outcome = run_flatten(target, &options)?;
        Ok(build_report(&outcome, dry_run))
    }
}

/// 把核心执行结果转成 toolbox-core 的中文报告。
fn build_report(outcome: &FlattenOutcome, dry_run: bool) -> ToolReport {
    let stats = &outcome.stats;
    let mode = if dry_run { "预览" } else { "处理" };
    let summary = format!(
        "文件夹平铺{}完成：找到文件 {}，移动 {}，重命名 {}，删除空文件夹 {}，跳过 {}，失败 {}",
        mode,
        stats.found_files,
        stats.moved,
        stats.renamed,
        stats.dirs_removed,
        stats.skipped,
        stats.failed
    );
    let mut report = if stats.failed == 0 {
        ToolReport::ok(summary)
    } else {
        ToolReport::fail(summary)
    };
    report.details.extend(outcome.details.iter().cloned());
    report
}
