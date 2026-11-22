//! tool-flatten 集成测试：全部使用临时目录，测试后自动清理。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tool_flatten::core::{run_flatten, ConflictPolicy, FlattenOptions};
use tool_flatten::FlattenTool;
use toolbox_core::{RunContext, Tool, ToolboxError};

fn options(dry_run: bool, on_conflict: ConflictPolicy) -> FlattenOptions {
    FlattenOptions {
        dry_run,
        on_conflict,
    }
}

/// 不依赖第三方 crate 的唯一临时目录，Drop 时递归删除。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let seq = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let name = format!(
            "tool-flatten-{}-{}-{}-{}",
            tag,
            std::process::id(),
            seq,
            nanos
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).expect("创建临时目录失败");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn tool_ctx(target: Option<&Path>, options_json: serde_json::Value) -> RunContext {
    RunContext {
        target: target.map(|p| p.to_path_buf()),
        options: options_json,
    }
}

#[test]
fn basic_flatten_moves_files_and_removes_empty_dirs() {
    let tmp = TempDir::new("basic");
    let root = tmp.path();
    fs::create_dir(root.join("sub1")).unwrap();
    fs::create_dir(root.join("sub2")).unwrap();
    fs::write(root.join("sub1/a.txt"), "A").unwrap();
    fs::write(root.join("sub1/b.txt"), "B").unwrap();
    fs::write(root.join("sub2/c.txt"), "C").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.found_files, 3);
    assert_eq!(outcome.stats.moved, 3);
    assert_eq!(outcome.stats.renamed, 0);
    assert_eq!(outcome.stats.dirs_removed, 2);
    assert_eq!(outcome.stats.skipped, 0);
    assert_eq!(outcome.stats.failed, 0);
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "A");
    assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "B");
    assert_eq!(fs::read_to_string(root.join("c.txt")).unwrap(), "C");
    assert!(!root.join("sub1").exists());
    assert!(!root.join("sub2").exists());
}

#[test]
fn auto_rename_same_names_from_multiple_subdirs() {
    let tmp = TempDir::new("rename");
    let root = tmp.path();
    fs::create_dir(root.join("a")).unwrap();
    fs::create_dir(root.join("b")).unwrap();
    fs::write(root.join("report.txt"), "original").unwrap();
    fs::write(root.join("a/report.txt"), "from-a").unwrap();
    fs::write(root.join("b/report.txt"), "from-b").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.moved, 2);
    assert_eq!(outcome.stats.renamed, 2);
    assert_eq!(fs::read_to_string(root.join("report.txt")).unwrap(), "original");
    assert_eq!(
        fs::read_to_string(root.join("report (1).txt")).unwrap(),
        "from-a"
    );
    assert_eq!(
        fs::read_to_string(root.join("report (2).txt")).unwrap(),
        "from-b"
    );
    assert!(!root.join("a").exists());
    assert!(!root.join("b").exists());
}

#[test]
fn skip_policy_keeps_source_and_non_empty_dir() {
    let tmp = TempDir::new("skip");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::create_dir(root.join("other")).unwrap();
    fs::write(root.join("dup.txt"), "root").unwrap();
    fs::write(root.join("sub/dup.txt"), "sub").unwrap();
    fs::write(root.join("other/free.txt"), "free").unwrap();

    let outcome = run_flatten(root, &options(false, ConflictPolicy::Skip)).unwrap();
    assert_eq!(outcome.stats.found_files, 2);
    assert_eq!(outcome.stats.moved, 1);
    assert_eq!(outcome.stats.skipped, 1);
    assert_eq!(outcome.stats.renamed, 0);
    assert_eq!(outcome.stats.dirs_removed, 1);
    assert_eq!(fs::read_to_string(root.join("dup.txt")).unwrap(), "root");
    assert_eq!(fs::read_to_string(root.join("sub/dup.txt")).unwrap(), "sub");
    assert!(root.join("sub").is_dir());
    assert!(!root.join("other").exists());
    assert_eq!(fs::read_to_string(root.join("free.txt")).unwrap(), "free");
    assert!(outcome.details.iter().any(|d| d.contains("跳过")));
}

#[test]
fn originally_empty_child_dir_is_removed() {
    let tmp = TempDir::new("empty");
    let root = tmp.path();
    fs::create_dir(root.join("empty")).unwrap();
    fs::create_dir(root.join("with_file")).unwrap();
    fs::write(root.join("with_file/a.txt"), "A").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.dirs_removed, 2);
    assert!(!root.join("empty").exists());
    assert!(!root.join("with_file").exists());
    assert!(root.join("a.txt").is_file());
}

#[test]
fn non_empty_child_dir_is_preserved() {
    let tmp = TempDir::new("keep");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::create_dir(root.join("sub/keep_me")).unwrap();
    fs::write(root.join("sub/move_me.txt"), "move").unwrap();
    fs::write(root.join("sub/keep_me/stay.txt"), "stay").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.moved, 1);
    assert_eq!(outcome.stats.dirs_removed, 0);
    assert!(root.join("sub").is_dir());
    assert!(root.join("sub/keep_me/stay.txt").is_file());
    assert!(root.join("move_me.txt").is_file());
    assert!(outcome
        .details
        .iter()
        .any(|d| d.contains("非空") && d.contains("保留")));
}

#[test]
fn dry_run_makes_no_changes() {
    let tmp = TempDir::new("dry");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("root.txt"), "root").unwrap();
    fs::write(root.join("sub/root.txt"), "sub").unwrap();
    fs::write(root.join("sub/new.txt"), "new").unwrap();

    let outcome = run_flatten(root, &options(true, ConflictPolicy::AutoRename)).unwrap();
    assert_eq!(outcome.stats.found_files, 2);
    assert_eq!(outcome.stats.moved, 0);
    assert_eq!(outcome.stats.renamed, 0);
    assert_eq!(outcome.stats.dirs_removed, 0);
    assert_eq!(fs::read_to_string(root.join("root.txt")).unwrap(), "root");
    assert_eq!(fs::read_to_string(root.join("sub/root.txt")).unwrap(), "sub");
    assert_eq!(fs::read_to_string(root.join("sub/new.txt")).unwrap(), "new");
    assert!(!root.join("root (1).txt").exists());
    assert!(!root.join("new.txt").exists());
    assert!(root.join("sub").is_dir());
    assert!(outcome.details.iter().any(|d| d.contains("预览")));
    assert!(outcome.details.iter().any(|d| d.contains("计划移动")));
}

#[test]
fn nested_subdir_is_not_processed() {
    let tmp = TempDir::new("nested");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::create_dir(root.join("sub/deep")).unwrap();
    fs::write(root.join("sub/top.txt"), "top").unwrap();
    fs::write(root.join("sub/deep/buried.txt"), "buried").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.found_files, 1);
    assert_eq!(outcome.stats.moved, 1);
    assert!(root.join("top.txt").is_file());
    assert!(root.join("sub/deep/buried.txt").is_file());
    assert!(root.join("sub").is_dir());
    assert_eq!(outcome.stats.dirs_removed, 0);
}

#[test]
fn missing_or_non_dir_target_returns_error() {
    let tmp = TempDir::new("missing");
    let missing = tmp.path().join("does-not-exist");
    let err = run_flatten(&missing, &FlattenOptions::default()).unwrap_err();
    assert!(matches!(err, ToolboxError::InvalidArgument(_)), "{:?}", err);

    let file = tmp.path().join("a-file.txt");
    fs::write(&file, "x").unwrap();
    let err2 = run_flatten(&file, &FlattenOptions::default()).unwrap_err();
    assert!(matches!(err2, ToolboxError::InvalidArgument(_)), "{:?}", err2);
}

#[test]
fn tool_returns_invalid_argument_when_target_missing() {
    let err = FlattenTool
        .run(tool_ctx(None, serde_json::json!({})))
        .unwrap_err();
    assert!(matches!(err, ToolboxError::InvalidArgument(_)), "{:?}", err);
}

#[test]
fn existing_file_is_never_overwritten() {
    let tmp = TempDir::new("overwrite");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("dup.txt"), "ORIGINAL").unwrap();
    fs::write(root.join("sub/dup.txt"), "NEW").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(outcome.stats.renamed, 1);
    assert_eq!(fs::read_to_string(root.join("dup.txt")).unwrap(), "ORIGINAL");
    assert_eq!(fs::read_to_string(root.join("dup (1).txt")).unwrap(), "NEW");
    assert!(!root.join("sub").exists());
}

#[test]
fn auto_rename_skips_already_taken_suffix() {
    let tmp = TempDir::new("suffix");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("dup.txt"), "0").unwrap();
    fs::write(root.join("dup (1).txt"), "1").unwrap();
    fs::write(root.join("sub/dup.txt"), "2").unwrap();

    let outcome = run_flatten(root, &FlattenOptions::default()).unwrap();
    assert_eq!(fs::read_to_string(root.join("dup (2).txt")).unwrap(), "2");
    assert_eq!(outcome.stats.moved, 1);
    assert_eq!(outcome.stats.renamed, 1);
}

#[test]
fn tool_parses_options_from_json() {
    let tmp = TempDir::new("options");
    let root = tmp.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("dup.txt"), "root").unwrap();
    fs::write(root.join("sub/dup.txt"), "sub").unwrap();

    let report = FlattenTool
        .run(tool_ctx(
            Some(root),
            serde_json::json!({"dry_run": true, "on_conflict": "skip"}),
        ))
        .unwrap();
    assert!(report.ok, "{:?}", report);
    assert_eq!(fs::read_to_string(root.join("sub/dup.txt")).unwrap(), "sub");
    assert!(!root.join("dup (1).txt").exists());

    let err = FlattenTool
        .run(tool_ctx(
            Some(root),
            serde_json::json!({"on_conflict": "overwrite"}),
        ))
        .unwrap_err();
    assert!(matches!(err, ToolboxError::InvalidArgument(_)), "{:?}", err);
}
