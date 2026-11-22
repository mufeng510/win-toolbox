//! 图片上下合并工具 crate。
//!
//! 实现 `toolbox_core::Tool`，核心算法与文件安全流程在 `core` 模块中。
//! 删除原图不可逆：先写临时文件、验证可解码且尺寸正确、重命名成功之后才删除原图。

pub mod core;

use std::path::PathBuf;

use toolbox_core::{Capabilities, RunContext, Tool, ToolReport, ToolboxError};

use crate::core::{merge_all, PairOutcome};

#[derive(Debug, Default, Clone, Copy)]
pub struct ImageMergeTool;

impl ImageMergeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ImageMergeTool {
    fn id(&self) -> &'static str {
        "image-merge"
    }

    fn name(&self) -> &'static str {
        "图片上下合并"
    }

    fn description(&self) -> &'static str {
        "按文件名自然排序后两两配对，把两张图片垂直拼接成一张；宽度必须一致，可设置重叠行数，输出格式跟随原图，验证成功后删除原图。"
    }

    fn platforms(&self) -> &'static [&'static str] {
        &["windows", "macos", "linux"]
    }

    fn capabilities(&self) -> Capabilities {
        // 右键菜单由外壳从 Tool trait 的 capabilities 统一生成，这里不重复注册。
        Capabilities::default()
    }

    fn run(&self, ctx: RunContext) -> toolbox_core::Result<ToolReport> {
        let paths = parse_paths(&ctx.options)?;

        // 规格键为 delete_sources；同时兼容项目既有的 delete_source / delete_original，
        // 避免前端传了「不删除」却被这里当成默认值 true 而误删原图。
        let delete_sources = ctx
            .options
            .get("delete_sources")
            .or_else(|| ctx.options.get("delete_source"))
            .or_else(|| ctx.options.get("delete_original"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

        let quality = ctx
            .options
            .get("quality")
            .and_then(|value| value.as_u64())
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(95);

        let overlap_usize = ctx
            .options
            .get("overlap")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        let overlap = u32::try_from(overlap_usize)
            .map_err(|_| ToolboxError::InvalidArgument("overlap 行数过大".to_string()))?;

        let (outcomes, skipped) = merge_all(paths, overlap, quality, delete_sources);
        build_report(&outcomes, &skipped, delete_sources)
    }
}

/// 从 options.paths 读取图片路径数组；空数组按参数错误处理。
fn parse_paths(options: &serde_json::Value) -> toolbox_core::Result<Vec<PathBuf>> {
    let array = options
        .get("paths")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            ToolboxError::InvalidArgument("缺少 paths 参数（需要图片路径的字符串数组）".to_string())
        })?;

    if array.is_empty() {
        return Err(ToolboxError::InvalidArgument(
            "paths 不能为空，至少需要两张图片".to_string(),
        ));
    }

    let mut paths = Vec::with_capacity(array.len());
    for (index, value) in array.iter().enumerate() {
        let text = value.as_str().ok_or_else(|| {
            ToolboxError::InvalidArgument(format!("paths[{index}] 不是字符串"))
        })?;
        if text.trim().is_empty() {
            return Err(ToolboxError::InvalidArgument(format!(
                "paths[{index}] 是空字符串"
            )));
        }
        paths.push(PathBuf::from(text));
    }
    Ok(paths)
}

/// 汇总所有配对结果。`ok` 当且仅当没有任何一对失败；奇数落单只说明、不算失败。
fn build_report(
    outcomes: &[PairOutcome],
    skipped: &[PathBuf],
    delete_sources: bool,
) -> toolbox_core::Result<ToolReport> {
    let failed_count = outcomes.iter().filter(|item| item.error.is_some()).count();
    let succeeded = outcomes.len() - failed_count;
    let mut details = Vec::new();

    details.push(format!(
        "删除原图：{}",
        if delete_sources {
            "是（仅在临时文件验证并重命名成功后才删除）"
        } else {
            "否（保留原图）"
        }
    ));

    for item in outcomes {
        if let Some(output) = &item.output {
            details.push(format!(
                "第 {} 对：{} + {} → {}",
                item.index,
                item.first.display(),
                item.second.display(),
                output.display()
            ));
            if !item.deleted.is_empty() {
                let deleted: Vec<String> = item
                    .deleted
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                details.push(format!("  已删除原图：{}", deleted.join("、")));
            }
        }
        if let Some(error) = &item.error {
            details.push(format!(
                "第 {} 对失败：{} + {}；原因：{}",
                item.index,
                item.first.display(),
                item.second.display(),
                error
            ));
            details.push("  该对原图未删除（任何一步失败都不会删除原图）".to_string());
        }
    }

    if !skipped.is_empty() {
        let names: Vec<String> = skipped
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        details.push(format!("奇数张图片，跳过未配对：{}", names.join("、")));
    }

    let message = if outcomes.is_empty() {
        format!("未形成任何配对，跳过 {} 张图片", skipped.len())
    } else if failed_count == 0 {
        format!("成功合并 {} 对，跳过 {} 张", succeeded, skipped.len())
    } else {
        format!(
            "成功合并 {} 对，失败 {} 对，跳过 {} 张",
            succeeded, failed_count, skipped.len()
        )
    };

    let mut report = if failed_count == 0 {
        ToolReport::ok(message)
    } else {
        ToolReport::fail(message)
    };
    report.details = details;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use image::{DynamicImage, Rgba, RgbaImage};

    use super::*;
    use crate::core::{
        choose_format, is_jpeg_path, merge_images, natural_cmp, natural_cmp_str, pair_paths,
        unique_output_path, OutputFormat,
    };

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    /// 测试用临时目录，析构时尽力清理。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let seq = TEST_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "imgmerge-test-{}-{nanos}-{seq}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Rgba<u8> {
        Rgba([red, green, blue, alpha])
    }

    fn write_solid_png(path: &Path, width: u32, height: u32, color: Rgba<u8>) {
        let image = RgbaImage::from_pixel(width, height, color);
        DynamicImage::ImageRgba8(image).save(path).unwrap();
    }

    fn paths_to_json(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn metadata_matches_spec() {
        let tool = ImageMergeTool::new();
        assert_eq!(tool.id(), "image-merge");
        assert_eq!(tool.name(), "图片上下合并");
        assert_eq!(tool.platforms(), &["windows", "macos", "linux"][..]);
        assert!(!tool.capabilities().system_tweak);
        assert!(tool.capabilities().context_menu.is_empty());
    }

    #[test]
    fn natural_order_puts_2_before_10() {
        assert_eq!(natural_cmp_str("img2.png", "img10.png"), Ordering::Less);
        assert_eq!(natural_cmp_str("img10.png", "img2.png"), Ordering::Greater);
        assert_eq!(natural_cmp_str("img2.png", "img2.png"), Ordering::Equal);
        assert_eq!(natural_cmp_str("img2.png", "img02.png"), Ordering::Less);
        assert_eq!(natural_cmp_str("a1b2.png", "a1b10.png"), Ordering::Less);
    }

    #[test]
    fn natural_cmp_uses_file_name_not_directory() {
        let a = Path::new("aaa").join("img2.png");
        let b = Path::new("bbb").join("img10.png");
        assert_eq!(natural_cmp(&a, &b), Ordering::Less);
    }

    #[test]
    fn pair_paths_sorts_and_pairs_even_and_odd() {
        let even = vec![
            PathBuf::from("img10.png"),
            PathBuf::from("img2.png"),
            PathBuf::from("img1.png"),
            PathBuf::from("img3.png"),
        ];
        let result = pair_paths(even);
        assert_eq!(result.skipped.len(), 0);
        assert_eq!(result.pairs.len(), 2);
        assert_eq!(result.pairs[0].0.file_name().unwrap().to_string_lossy(), "img1.png");
        assert_eq!(result.pairs[0].1.file_name().unwrap().to_string_lossy(), "img2.png");
        assert_eq!(result.pairs[1].0.file_name().unwrap().to_string_lossy(), "img3.png");
        assert_eq!(result.pairs[1].1.file_name().unwrap().to_string_lossy(), "img10.png");

        let odd = vec![
            PathBuf::from("img10.png"),
            PathBuf::from("img2.png"),
            PathBuf::from("img1.png"),
        ];
        let result = pair_paths(odd);
        assert_eq!(result.pairs.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].file_name().unwrap().to_string_lossy(), "img10.png");
    }

    #[test]
    fn merge_rejects_different_widths() {
        let first = DynamicImage::ImageRgba8(RgbaImage::new(4, 3));
        let second = DynamicImage::ImageRgba8(RgbaImage::new(5, 3));
        let error = merge_images(&first, &second, 0).unwrap_err();
        assert!(error.to_string().contains("宽度不同"));
    }

    #[test]
    fn merge_produces_vertical_image_in_correct_order() {
        let red = rgba(255, 0, 0, 255);
        let green = rgba(0, 255, 0, 255);
        let blue = rgba(0, 0, 255, 255);
        let white = rgba(255, 255, 255, 255);
        let black = rgba(0, 0, 0, 255);

        let first = RgbaImage::from_fn(1, 3, |_x, y| match y {
            0 => red,
            1 => green,
            _ => blue,
        });
        let second = RgbaImage::from_fn(1, 2, |_x, y| if y == 0 { white } else { black });

        let merged = merge_images(
            &DynamicImage::ImageRgba8(first),
            &DynamicImage::ImageRgba8(second),
            0,
        )
        .unwrap();

        assert_eq!((merged.width(), merged.height()), (1, 5));
        assert_eq!(*merged.get_pixel(0, 0), red);
        assert_eq!(*merged.get_pixel(0, 1), green);
        assert_eq!(*merged.get_pixel(0, 2), blue);
        assert_eq!(*merged.get_pixel(0, 3), white);
        assert_eq!(*merged.get_pixel(0, 4), black);
    }

    #[test]
    fn overlap_drops_specified_rows_of_second_image() {
        let colors = [
            rgba(1, 0, 0, 255),
            rgba(2, 0, 0, 255),
            rgba(3, 0, 0, 255),
        ];
        let second_colors = [
            rgba(9, 0, 0, 255),
            rgba(8, 0, 0, 255),
            rgba(7, 0, 0, 255),
        ];
        let first = RgbaImage::from_fn(1, 3, |_x, y| colors[y as usize]);
        let second = RgbaImage::from_fn(1, 3, |_x, y| second_colors[y as usize]);

        let merged = merge_images(
            &DynamicImage::ImageRgba8(first),
            &DynamicImage::ImageRgba8(second),
            1,
        )
        .unwrap();

        assert_eq!((merged.width(), merged.height()), (1, 5));
        assert_eq!(*merged.get_pixel(0, 2), colors[2]);
        assert_eq!(*merged.get_pixel(0, 3), second_colors[1]);
        assert_eq!(*merged.get_pixel(0, 4), second_colors[2]);

        // 重叠行数不得大于或等于第二张高度。
        assert!(merge_images(
            &DynamicImage::ImageRgba8(RgbaImage::new(1, 3)),
            &DynamicImage::ImageRgba8(RgbaImage::new(1, 3)),
            3
        )
        .is_err());
    }

    #[test]
    fn format_selection_and_extension() {
        assert_eq!(choose_format(true, true), OutputFormat::Jpeg);
        assert_eq!(choose_format(true, false), OutputFormat::Png);
        assert_eq!(choose_format(false, true), OutputFormat::Png);
        assert_eq!(choose_format(false, false), OutputFormat::Png);
        assert_eq!(OutputFormat::Jpeg.extension(), "jpg");
        assert_eq!(OutputFormat::Png.extension(), "png");
        assert!(is_jpeg_path(Path::new("a.JPG")));
        assert!(is_jpeg_path(Path::new("a.jpeg")));
        assert!(!is_jpeg_path(Path::new("a.png")));
    }

    #[test]
    fn unique_output_path_never_overwrites() {
        let dir = TempDir::new();
        fs::write(dir.join("pic_合并.png"), b"old").unwrap();
        fs::write(dir.join("pic_合并 (1).png"), b"old").unwrap();

        assert_eq!(
            unique_output_path(dir.path(), "pic_合并", "png"),
            dir.join("pic_合并 (2).png")
        );
        assert_eq!(fs::read(dir.join("pic_合并.png")).unwrap(), b"old".to_vec());
    }

    #[test]
    fn full_flow_writes_outputs_and_deletes_sources() {
        let dir = TempDir::new();
        let names = ["img1.png", "img2.png", "img3.png", "img4.png"];
        let sources: Vec<PathBuf> = names.iter().map(|name| dir.join(name)).collect();
        for source in &sources {
            write_solid_png(source, 4, 3, rgba(10, 20, 30, 255));
        }

        let ctx = RunContext {
            target: None,
            options: serde_json::json!({
                "paths": paths_to_json(&sources),
                "delete_sources": true,
                "quality": 95,
                "overlap": 0,
            }),
        };
        let report = ImageMergeTool::new().run(ctx).unwrap();

        assert!(report.ok, "报告失败：{} / {:?}", report.message, report.details);
        let first_output = dir.join("img1_合并.png");
        let second_output = dir.join("img3_合并.png");
        assert!(first_output.exists());
        assert!(second_output.exists());
        for source in &sources {
            assert!(!source.exists(), "原图应已删除：{}", source.display());
        }
        assert!(report.details.iter().any(|line| line.contains("已删除原图")));

        let decoded = image::open(&first_output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 6));
    }

    #[test]
    fn failed_pair_keeps_all_source_images() {
        let dir = TempDir::new();
        let first = dir.join("a.png");
        let second = dir.join("b.png");
        write_solid_png(&first, 4, 3, rgba(1, 1, 1, 255));
        write_solid_png(&second, 5, 3, rgba(2, 2, 2, 255));

        let ctx = RunContext {
            target: None,
            options: serde_json::json!({
                "paths": paths_to_json(&[first.clone(), second.clone()]),
                "delete_sources": true,
            }),
        };
        let report = ImageMergeTool::new().run(ctx).unwrap();

        assert!(!report.ok);
        assert!(report.details.iter().any(|line| line.contains("宽度不同")));
        assert!(first.exists());
        assert!(second.exists());
        assert!(!dir.join("a_合并.png").exists());

        // 不应遗留任何临时文件。
        let leftovers: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| !path.ends_with("a.png") && !path.ends_with("b.png"))
            .collect();
        assert!(leftovers.is_empty(), "遗留文件：{leftovers:?}");
    }

    #[test]
    fn delete_sources_false_keeps_originals() {
        let dir = TempDir::new();
        let first = dir.join("x1.png");
        let second = dir.join("x2.png");
        write_solid_png(&first, 2, 2, rgba(3, 3, 3, 255));
        write_solid_png(&second, 2, 2, rgba(4, 4, 4, 255));

        let ctx = RunContext {
            target: None,
            options: serde_json::json!({
                "paths": paths_to_json(&[first.clone(), second.clone()]),
                "delete_sources": false,
            }),
        };
        let report = ImageMergeTool::new().run(ctx).unwrap();

        assert!(report.ok, "{}", report.message);
        assert!(first.exists());
        assert!(second.exists());
        assert!(dir.join("x1_合并.png").exists());
    }

    #[test]
    fn odd_number_of_paths_reports_skipped_image() {
        let dir = TempDir::new();
        let first = dir.join("img1.png");
        let second = dir.join("img2.png");
        let third = dir.join("img3.png");
        for path in [&first, &second, &third] {
            write_solid_png(path, 2, 2, rgba(5, 5, 5, 255));
        }

        let ctx = RunContext {
            target: None,
            options: serde_json::json!({
                "paths": paths_to_json(&[first.clone(), second.clone(), third.clone()]),
                "delete_sources": false,
            }),
        };
        let report = ImageMergeTool::new().run(ctx).unwrap();

        assert!(report.ok);
        assert!(dir.join("img1_合并.png").exists());
        assert!(third.exists());
        assert!(report.details.iter().any(|line| line.contains("跳过未配对")));
    }

    #[test]
    fn empty_paths_is_invalid_argument() {
        let ctx = RunContext {
            target: None,
            options: serde_json::json!({ "paths": [] }),
        };
        let error = ImageMergeTool::new().run(ctx).unwrap_err();
        assert!(matches!(error, ToolboxError::InvalidArgument(_)));
    }
}
