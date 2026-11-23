//! HashRename 核心业务逻辑移植(不含 GUI / CLI / Tauri 壳 / 右键菜单注册)。
//!
//! 对外只暴露 [`HashRenameTool`](struct.HashRenameTool.html)(实现 toolbox-core
//! 的 `Tool` trait)与 [`run_with_progress`](fn.run_with_progress.html),
//! 供统一 exe / GUI 调用。核心算法保留在 `core` 模块中。

pub mod core;
pub mod platform;

use std::path::Path;
use std::sync::{Arc, Mutex};

use toolbox_core::{Capabilities, MenuArg, MenuLocation, MenuVerb, RunContext, Tool, ToolReport, ToolboxError};

use crate::core::hasher::{default_thread_count, HashAlgorithm};
use crate::core::models::ProcessingResult;
use crate::core::processor::{process_directory, ProcessOptions};
use crate::core::progress::{Progress, ProgressEvent};
use crate::core::trash::OsTrash;

/// 从 `ctx.options` 解析出的工具配置。
#[derive(Debug, Clone)]
pub struct HashOptions {
    /// 哈希算法:`md5`(默认)/ `sha256` / `xxh3`。
    pub hash: String,
    /// 预览模式:零修改,只输出执行计划。
    pub dry_run: bool,
    /// 详细输出:在报告 details 中列出全部计划重命名。
    pub verbose: bool,
}

impl Default for HashOptions {
    fn default() -> Self {
        HashOptions {
            hash: "md5".to_string(),
            dry_run: false,
            verbose: false,
        }
    }
}

/// toolbox-core 工具实现。
pub struct HashRenameTool;

impl Tool for HashRenameTool {
    fn id(&self) -> &'static str {
        "hash-rename"
    }

    fn name(&self) -> &'static str {
        "HashRename 去重重命名"
    }

    fn description(&self) -> &'static str {
        "对当前文件夹做三重内容去重(大小 + 哈希 + 逐字节验证),重复文件移入回收站,剩余文件按自然排序统一编号重命名"
    }

    fn platforms(&self) -> &'static [&'static str] {
        &[]
    }

    fn capabilities(&self) -> Capabilities {
        // 右键菜单是工具的固有能力，由 Tool trait 声明，外壳从注册表统一生成。
        let label = self.name().to_string();
        Capabilities {
            context_menu: vec![
                MenuVerb {
                    location: MenuLocation::Directory,
                    label: label.clone(),
                    arg: MenuArg::PathV,
                },
                MenuVerb {
                    location: MenuLocation::DirectoryBackground,
                    label,
                    arg: MenuArg::PathV,
                },
            ],
            system_tweak: false,
        }
    }

    fn run(&self, ctx: RunContext) -> toolbox_core::Result<ToolReport> {
        let target = ctx
            .target
            .as_deref()
            .ok_or_else(|| ToolboxError::InvalidArgument("缺少目标文件夹".to_string()))?;

        let hash = ctx
            .options
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or("md5")
            .to_string();
        let dry_run = ctx
            .options
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let verbose = ctx
            .options
            .get("verbose")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        run_with_progress(target, &HashOptions { hash, dry_run, verbose }, |_| {})
    }
}

/// 带进度回调的执行入口(供 GUI 显示进度)。
///
/// `on_progress` 会在执行线程产生的每个阶段/警告/当前文件/百分比事件上被调用,
/// 参数是中文可读字符串。`Tool::run` 内部使用空闭包调用本函数。
///
/// 进度事件通过 channel 回传到调用线程后再调用 `on_progress`,
/// 因此 `on_progress` 不必是 `'static`,也不要求 `Send + Sync`。
pub fn run_with_progress(
    target: &Path,
    options: &HashOptions,
    on_progress: impl Fn(&str),
) -> toolbox_core::Result<ToolReport> {
    let algorithm = HashAlgorithm::parse(&options.hash).ok_or_else(|| {
        ToolboxError::InvalidArgument(format!(
            "不支持的哈希算法: {} (可选: md5 / sha256 / xxh3)",
            options.hash
        ))
    })?;

    let proc_options = ProcessOptions {
        hash_threads: default_thread_count(),
        hash_algorithm: algorithm,
        dry_run: options.dry_run,
    };

    let target_buf = target.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx = Arc::new(Mutex::new(tx));

    // 在独立线程跑核心流水线,把进度事件经 channel 送回本线程回调,
    // 这样 on_progress 可以借用局部变量(无需 'static)。
    let worker = std::thread::spawn(move || {
        let tx_in_worker = tx.clone();
        let handle = Progress::create(move |event: &ProgressEvent| {
            let message = match event {
                ProgressEvent::Stage { stage, message } => {
                    format!("[{:?}] {}", stage, message)
                }
                ProgressEvent::Counts {
                    scanned,
                    duplicates,
                    kept,
                } => format!(
                    "扫描 {} 个文件,检出重复 {} 个,保留 {} 个",
                    scanned, duplicates, kept
                ),
                ProgressEvent::CurrentFile { file } => format!("当前文件: {}", file),
                ProgressEvent::Progress { percent } => format!("进度: {:.1}%", percent),
                ProgressEvent::Warning { message } => format!("警告: {}", message),
                ProgressEvent::Finished { .. } => "处理完成".to_string(),
            };
            if let Ok(tx) = tx_in_worker.lock() {
                let _ = tx.send(message);
            }
        });
        process_directory(
            &target_buf,
            &proc_options,
            &handle.reporter(),
            Arc::new(OsTrash),
        )
    });

    // 收完所有消息(channel 在 worker 结束、tx 全部 drop 后关闭)。
    for message in rx {
        on_progress(&message);
    }

    let result = worker
        .join()
        .map_err(|_| ToolboxError::Tool("处理线程异常退出".to_string()))?
        .map_err(|e| ToolboxError::Tool(e.to_string()))?;

    Ok(report_from_result(&result, options.verbose))
}

/// 把核心结果转成 toolbox-core 的报告。
fn report_from_result(result: &ProcessingResult, verbose: bool) -> ToolReport {
    let mode = if result.dry_run { "预览" } else { "处理" };
    let mut report = if result.ok() {
        ToolReport::ok(format!(
            "HashRename {}完成: 扫描 {}, 重复 {} 个/{} 组, 移入回收站 {}, 重命名 {}",
            mode,
            result.scanned_count,
            result.duplicate_count,
            result.duplicate_groups,
            result.trashed_count,
            result.renamed_count
        ))
    } else {
        ToolReport::fail(format!(
            "HashRename {}完成但有 {} 项异常: 扫描 {}, 重复 {}, 移入回收站 {}, 重命名 {}",
            mode,
            result.failed_count,
            result.scanned_count,
            result.duplicate_count,
            result.trashed_count,
            result.renamed_count
        ))
    };

    report.details.push(format!(
        "扫描 {} 个普通文件,跳过 {} 个非普通文件,使用哈希算法 {}",
        result.scanned_count, result.skipped_count, result.hash_algorithm
    ));
    report.details.push(format!(
        "去重后保留 {} 个,成功重命名 {} 个,失败 {} 个,耗时 {} ms",
        result.kept_count, result.renamed_count, result.failed_count, result.elapsed_ms
    ));

    if result.dry_run {
        report
            .details
            .push("预览模式:未创建目录锁、未恢复历史任务、未修改任何文件".to_string());
        report.details.push(format!(
            "计划移入回收站({}): {}",
            result.planned_trashes.len(),
            result.planned_trashes.join(", ")
        ));
    }
    if verbose && !result.planned_renames.is_empty() {
        report.details.push(format!(
            "计划重命名({}):",
            result.planned_renames.len()
        ));
        for rename in &result.planned_renames {
            report
                .details
                .push(format!("  {} -> {}", rename.from, rename.to));
        }
    }
    if result.recovered_finals > 0 || result.restored_temps > 0 {
        report.details.push(format!(
            "崩溃恢复:补完 {} 个重命名,还原 {} 个临时文件",
            result.recovered_finals, result.restored_temps
        ));
    }
    if result.orphan_temps > 0 {
        report.details.push(format!(
            "发现 {} 个无法识别来源的遗留临时文件(已排除)",
            result.orphan_temps
        ));
    }
    for error in &result.errors {
        report.details.push(format!(
            "错误[{}] {}: {}",
            error.operation, error.path, error.message
        ));
    }
    for warning in &result.warnings {
        report.details.push(format!("警告: {}", warning));
    }
    if result.cancelled {
        report.details.push("任务已取消".to_string());
    }

    report
}
