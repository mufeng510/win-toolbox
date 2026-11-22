//! 文件夹平铺的核心逻辑：只处理目标文件夹的直接子文件夹，
//! 把其中的普通文件移动到目标文件夹根目录，并处理重名与空目录清理。
//!
//! 本模块不依赖 GUI / 右键菜单 / 配置存储，便于单独单元测试。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use toolbox_core::ToolboxError;

/// 目标路径已存在时的处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// 在扩展名前追加 " (1)"、" (2)"……直到不冲突（默认，绝不覆盖）。
    AutoRename,
    /// 跳过该文件，并在报告中列出。
    Skip,
}

impl ConflictPolicy {
    /// 解析配置字符串；未知值返回 None，由调用方报告为参数错误。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto_rename" => Some(Self::AutoRename),
            "skip" => Some(Self::Skip),
            _ => None,
        }
    }
}

/// 从 ctx.options 解析出的执行选项。
#[derive(Debug, Clone, Copy)]
pub struct FlattenOptions {
    /// 预览模式：零修改，只生成计划并写入报告详情。
    pub dry_run: bool,
    /// 重名处理策略。
    pub on_conflict: ConflictPolicy,
}

impl Default for FlattenOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            on_conflict: ConflictPolicy::AutoRename,
        }
    }
}

/// 单条移动计划。
#[derive(Debug, Clone)]
pub struct MovePlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    /// 是否因重名而改过文件名。
    pub renamed: bool,
}

/// 单条跳过记录。
#[derive(Debug, Clone)]
pub struct SkipRecord {
    pub source: PathBuf,
}

/// 扫描阶段生成的完整计划。
#[derive(Debug, Default)]
pub struct FlattenPlan {
    pub moves: Vec<MovePlan>,
    pub skips: Vec<SkipRecord>,
    /// 目标文件夹的所有直接子文件夹（含原本就空的），用于事后清理。
    pub child_dirs: Vec<PathBuf>,
}

/// 执行统计。
#[derive(Debug, Default, Clone)]
pub struct ExecutionStats {
    /// 找到的普通文件数（含被跳过的）。
    pub found_files: usize,
    /// 实际移动成功数。
    pub moved: usize,
    /// 实际重命名数（auto_rename 生效的文件数）。
    pub renamed: usize,
    /// 实际删除的空子文件夹数。
    pub dirs_removed: usize,
    /// 因重名策略跳过的文件数。
    pub skipped: usize,
    /// 移动 / 删除失败数。
    pub failed: usize,
}

/// 一次完整执行的结果，供外壳层生成 ToolReport。
#[derive(Debug, Default)]
pub struct FlattenOutcome {
    pub stats: ExecutionStats,
    pub details: Vec<String>,
}

/// 批量清理空目录的结果。
#[derive(Debug, Default)]
pub struct DirCleanup {
    pub removed: usize,
    pub kept: usize,
    pub failed: usize,
    pub details: Vec<String>,
}

/// 路径转中文报告用的显示字符串。
fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// 冲突判定的键：只取文件名并统一小写。
/// Windows / macOS 默认文件系统大小写不敏感，统一小写可避免
/// "A.txt" 覆盖 "a.txt"；Linux 上则偏保守（可能多改名一次，但不会丢数据）。
pub fn conflict_key(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 把文件名拆成（主名, 扩展名）。".gitignore" 这类以点开头且无第二个点的
/// 文件视为无扩展名；"a.tar.gz" 的扩展名取最后的 "gz"。
fn split_name(path: &Path) -> (String, Option<String>) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), Some(name[i + 1..].to_string())),
        _ => (name, None),
    }
}

/// 计算一个不与 `occupied` 冲突的目标路径，并把该路径登记进 `occupied`。
///
/// 返回 `(最终路径, 是否发生过重命名)`；当策略为 `Skip` 且期望路径被占用时返回 `None`。
/// 调用方应先用目标文件夹根目录的所有条目初始化 `occupied`，
/// 这样后续同一批文件之间也不会互相覆盖。
pub fn unique_destination(
    desired: &Path,
    occupied: &mut HashSet<String>,
    policy: ConflictPolicy,
) -> Option<(PathBuf, bool)> {
    let key = conflict_key(desired);
    if !occupied.contains(&key) {
        occupied.insert(key);
        return Some((desired.to_path_buf(), false));
    }
    if policy == ConflictPolicy::Skip {
        return None;
    }

    let (stem, ext) = split_name(desired);
    let mut index: u64 = 1;
    let chosen = loop {
        let file_name = match &ext {
            Some(e) => format!("{} ({}).{}", stem, index, e),
            None => format!("{} ({})", stem, index),
        };
        let candidate = desired.with_file_name(file_name);
        if !occupied.contains(&conflict_key(&candidate)) {
            break candidate;
        }
        index += 1;
    };
    occupied.insert(conflict_key(&chosen));
    Some((chosen, true))
}

/// 扫描目标文件夹并生成计划，不修改任何文件。
///
/// 目标不存在 / 不是目录时返回 `ToolboxError::InvalidArgument`。
pub fn plan_flatten(target: &Path, options: &FlattenOptions) -> toolbox_core::Result<FlattenPlan> {
    if !target.is_dir() {
        return Err(ToolboxError::InvalidArgument(format!(
            "目标文件夹不存在或不是目录: {}",
            display_path(target)
        )));
    }

    let mut occupied: HashSet<String> = HashSet::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        // 根目录的所有条目（含子文件夹）都当作已占用，避免把文件移到同名目录上。
        occupied.insert(conflict_key(&entry.path()));
        // 用 file_type() 而不是 path.is_dir()，避免跟随符号链接 / 目录联接。
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();

    let mut plan = FlattenPlan::default();
    for dir in &dirs {
        let mut files: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            // 只处理普通文件；子目录与符号链接都不移动、也不删除。
            if entry.file_type()?.is_file() {
                files.push(entry.path());
            }
        }
        files.sort();

        for file in files {
            let file_name = file
                .file_name()
                .map(|n| PathBuf::from(n))
                .unwrap_or_default();
            let desired = target.join(file_name);
            match unique_destination(&desired, &mut occupied, options.on_conflict) {
                Some((destination, renamed)) => {
                    plan.moves.push(MovePlan {
                        source: file,
                        destination,
                        renamed,
                    });
                }
                None => plan.skips.push(SkipRecord { source: file }),
            }
        }
    }
    plan.child_dirs = dirs;
    Ok(plan)
}

/// 移动单个文件：先试 `fs::rename`，失败则「复制到目标 + 删除源文件」。
///
/// 任何失败都会清理已复制的目标文件，不留下半成品；
/// 目标已存在时直接报错，绝不覆盖。
pub fn move_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标文件已存在，拒绝覆盖: {}", display_path(destination)),
        ));
    }

    if fs::rename(source, destination).is_ok() {
        return Ok(());
    }

    if let Err(err) = fs::copy(source, destination) {
        let _ = fs::remove_file(destination);
        return Err(err);
    }
    if let Err(err) = fs::remove_file(source) {
        let _ = fs::remove_file(destination);
        return Err(err);
    }
    Ok(())
}

/// 目录为空则删除并返回 true；非空返回 false；读取 / 删除失败返回 Err。
pub fn remove_empty_dir(dir: &Path) -> std::io::Result<bool> {
    let mut entries = fs::read_dir(dir)?;
    if entries.next().is_some() {
        return Ok(false);
    }
    fs::remove_dir(dir)?;
    Ok(true)
}

/// 批量检查并删除空子文件夹（原本就空的也会被删除）。
pub fn remove_empty_dirs(dirs: &[PathBuf]) -> DirCleanup {
    let mut cleanup = DirCleanup::default();
    for dir in dirs {
        match remove_empty_dir(dir) {
            Ok(true) => {
                cleanup.removed += 1;
                cleanup
                    .details
                    .push(format!("已删除空子文件夹：{}", display_path(dir)));
            }
            Ok(false) => {
                cleanup.kept += 1;
                cleanup.details.push(format!(
                    "子文件夹非空（仍有文件或子目录），已保留：{}",
                    display_path(dir)
                ));
            }
            Err(err) => {
                cleanup.failed += 1;
                cleanup.details.push(format!(
                    "删除子文件夹失败：{}（{}）",
                    display_path(dir),
                    err
                ));
            }
        }
    }
    cleanup
}

/// 完整执行：先规划，再按模式生成预览或真正移动 / 清理。
pub fn run_flatten(target: &Path, options: &FlattenOptions) -> toolbox_core::Result<FlattenOutcome> {
    let plan = plan_flatten(target, options)?;
    let mut stats = ExecutionStats {
        found_files: plan.moves.len() + plan.skips.len(),
        skipped: plan.skips.len(),
        ..Default::default()
    };
    let mut details = Vec::new();

    details.push(format!(
        "发现直接子文件夹 {} 个，普通文件 {} 个。",
        plan.child_dirs.len(),
        stats.found_files
    ));

    if options.dry_run {
        details.push(
            "预览模式（dry_run）：仅生成计划，未移动文件、未删除文件夹、未创建任何锁。".to_string(),
        );
        for mv in &plan.moves {
            let tag = if mv.renamed { "（将重命名）" } else { "" };
            details.push(format!(
                "计划移动{}：{} → {}",
                tag,
                display_path(&mv.source),
                display_path(&mv.destination)
            ));
        }
        for sk in &plan.skips {
            details.push(format!(
                "计划跳过：{}（目标同名文件已存在，策略为 skip）",
                display_path(&sk.source)
            ));
        }
        for dir in &plan.child_dirs {
            details.push(format!("计划检查并删除空子文件夹：{}", display_path(dir)));
        }
        return Ok(FlattenOutcome { stats, details });
    }

    for mv in &plan.moves {
        match move_file(&mv.source, &mv.destination) {
            Ok(()) => {
                stats.moved += 1;
                if mv.renamed {
                    stats.renamed += 1;
                }
                details.push(format!(
                    "已移动：{} → {}",
                    display_path(&mv.source),
                    display_path(&mv.destination)
                ));
            }
            Err(err) => {
                stats.failed += 1;
                details.push(format!(
                    "移动失败：{} → {}（{}）",
                    display_path(&mv.source),
                    display_path(&mv.destination),
                    err
                ));
            }
        }
    }
    for sk in &plan.skips {
        details.push(format!(
            "已跳过：{}（目标同名文件已存在，策略为 skip）",
            display_path(&sk.source)
        ));
    }

    let cleanup = remove_empty_dirs(&plan.child_dirs);
    stats.dirs_removed = cleanup.removed;
    stats.failed += cleanup.failed;
    details.extend(cleanup.details);

    Ok(FlattenOutcome { stats, details })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_destination_handles_hidden_files() {
        let mut occupied = HashSet::new();
        occupied.insert(conflict_key(Path::new(".gitignore")));
        let (dest, renamed) = unique_destination(
            Path::new("/tmp/.gitignore"),
            &mut occupied,
            ConflictPolicy::AutoRename,
        )
        .unwrap();
        assert!(renamed);
        assert_eq!(
            dest.file_name().unwrap().to_string_lossy().as_ref(),
            ".gitignore (1)"
        );
    }

    #[test]
    fn unique_destination_without_extension() {
        let mut occupied = HashSet::new();
        occupied.insert(conflict_key(Path::new("README")));
        let (dest, renamed) = unique_destination(
            Path::new("/tmp/README"),
            &mut occupied,
            ConflictPolicy::AutoRename,
        )
        .unwrap();
        assert!(renamed);
        assert_eq!(
            dest.file_name().unwrap().to_string_lossy().as_ref(),
            "README (1)"
        );
    }
}
