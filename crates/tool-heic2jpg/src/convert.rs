//! 批量 HEIC/HEIF → JPG 的核心逻辑。
//!
//! 把所有可单测的判断逻辑（文件收集、跳过规则、参数解析、EXIF 方向、
//! JPEG 校验）拆成独立函数，这样即使没有任何真实的 HEIC 样本，
//! 也能用 `cargo test -p tool-heic2jpg` 覆盖主要规则。
//! 这里刻意不依赖 GUI / 命令行参数，原 C# 版的 `--probe` / `--selftest`
//! 思路在本 crate 中体现为「纯函数 + 单元测试」。

use std::path::{Path, PathBuf};

use image::metadata::Orientation;
use serde_json::Value;
use toolbox_core::{Result, ToolboxError};

/// 默认 JPG 质量，与原 C# 版一致。
pub const DEFAULT_QUALITY: u8 = 95;
pub const MIN_QUALITY: u8 = 1;
pub const MAX_QUALITY: u8 = 100;

/// 支持的源扩展名（小写，比较时忽略大小写）。
pub const HEIC_EXTENSIONS: [&str; 2] = ["heic", "heif"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvertOptions {
    pub quality: u8,
    pub recursive: bool,
    pub delete_source: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            quality: DEFAULT_QUALITY,
            recursive: true,
            delete_source: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ConvertStats {
    pub total: usize,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes_out: u64,
    pub bytes_freed: u64,
}

#[derive(Debug, Default)]
pub struct ConvertOutcome {
    pub stats: ConvertStats,
    pub logs: Vec<String>,
}

/// 把 JSON 里任意数值收敛到 1..=100；非有限数用默认值。
fn clamp_quality(value: f64) -> u8 {
    if !value.is_finite() {
        return DEFAULT_QUALITY;
    }
    value.round().clamp(MIN_QUALITY as f64, MAX_QUALITY as f64) as u8
}

/// 从 `ctx.options` 解析参数，缺失/类型错误时回落到原 C# 版的默认值。
pub fn build_options(options: &Value) -> ConvertOptions {
    let quality = options
        .get("quality")
        .and_then(|v| {
            if let Some(i) = v.as_i64() {
                Some(clamp_quality(i as f64))
            } else if let Some(u) = v.as_u64() {
                Some(clamp_quality(u as f64))
            } else {
                v.as_f64().map(clamp_quality)
            }
        })
        .unwrap_or(DEFAULT_QUALITY);

    let recursive = options
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let delete_source = options
        .get("delete_source")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    ConvertOptions {
        quality,
        recursive,
        delete_source,
    }
}

/// 扩展名是否为 heic / heif（忽略大小写）。
pub fn is_heic(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| HEIC_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(*x)))
        .unwrap_or(false)
}

/// 输出与原文件同目录、同主文件名，仅把扩展名换成 jpg。
pub fn target_path(source: &Path) -> PathBuf {
    source.with_extension("jpg")
}

/// 递归收集 HEIC/HEIF 文件；跳过符号链接以避免目录环。
/// 排序规则与原 C# 版一致：按路径字符串大小写不敏感排序。
pub fn collect_targets(dir: &Path, recursive: bool) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_into(dir, recursive, &mut files)?;
    files.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });
    files.dedup();
    Ok(files)
}

fn collect_into(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        // 不跟随符号链接：既能避免目录环，也避免把用户链接到别处的文件删掉。
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if recursive {
                collect_into(&path, recursive, out)?;
            }
        } else if kind.is_file() && is_heic(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// 同名 JPG 已存在且非空 → 跳过（且不要删除对应 HEIC）。
/// 与原 C# 版一致：零字节的 JPG 不算「已存在」。
pub fn should_skip(dest: &Path) -> bool {
    match std::fs::metadata(dest) {
        Ok(meta) => meta.is_file() && meta.len() > 0,
        Err(_) => false,
    }
}

/// 兼容 exif chunk 的 `Exif\0\0` 前缀与裸 TIFF 数据。
fn tiff_bytes(exif: &[u8]) -> &[u8] {
    if exif.len() >= 6 && &exif[..6] == &b"Exif\0\0"[..] {
        &exif[6..]
    } else {
        exif
    }
}

/// 从原始 EXIF/TIFF 字节中解析 IFD0 的 Orientation（tag 0x0112）。
/// 这是纯函数，可用人工构造的 TIFF 字节直接单测。
pub fn parse_exif_orientation(exif: &[u8]) -> Option<u8> {
    let tiff = tiff_bytes(exif);
    if tiff.len() < 8 {
        return None;
    }

    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };

    let u16_at = |buf: &[u8]| -> Option<u16> {
        if buf.len() < 2 {
            return None;
        }
        Some(if little {
            u16::from_le_bytes([buf[0], buf[1]])
        } else {
            u16::from_be_bytes([buf[0], buf[1]])
        })
    };
    let u32_at = |buf: &[u8]| -> Option<u32> {
        if buf.len() < 4 {
            return None;
        }
        Some(if little {
            u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
        } else {
            u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
        })
    };

    if u16_at(&tiff[2..4])? != 42 {
        return None;
    }
    let ifd = u32_at(&tiff[4..8])? as usize;
    let count = u16_at(tiff.get(ifd..ifd.saturating_add(2))?)? as usize;
    // 每个 IFD entry 12 字节；数量必须落在文件范围内。
    if count > tiff.len().saturating_sub(ifd.saturating_add(2)) / 12 {
        return None;
    }

    for i in 0..count {
        let start = ifd.saturating_add(2).saturating_add(i.saturating_mul(12));
        let entry = tiff.get(start..start.saturating_add(12))?;
        let tag = u16_at(&entry[0..2])?;
        if tag != 0x0112 {
            continue;
        }
        // Orientation 类型为 SHORT(3)，count 通常为 1；值直接放在 value 域前 2 字节。
        let ty = u16_at(&entry[2..4])?;
        let n = u32_at(&entry[4..8])?;
        if ty != 3 || n < 1 {
            return None;
        }
        let value = u16_at(&entry[8..10])?;
        return if (1..=8).contains(&value) {
            Some(value as u8)
        } else {
            None
        };
    }
    None
}

/// 优先使用 image crate 的 chunk 解析，失败时回落到自写的 TIFF 解析。
fn orientation_from_exif(exif: &[u8]) -> Orientation {
    Orientation::from_exif_chunk(exif)
        .or_else(|| parse_exif_orientation(exif).and_then(Orientation::from_exif))
        .unwrap_or(Orientation::NoTransforms)
}

#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 用 heic 的纯 Rust 后端解码为 8bit RGBA。
pub fn decode_heic(
    config: &heic::DecoderConfig,
    bytes: &[u8],
) -> std::result::Result<DecodedImage, String> {
    let output = config
        .decode(bytes, heic::PixelLayout::Rgba8)
        .map_err(|e| format!("HEIC 解码失败: {e:?}"))?;
    Ok(DecodedImage {
        width: output.width,
        height: output.height,
        rgba: output.data,
    })
}

/// JPEG 编码。使用明确的 RGB8 输入，避免 alpha 通道进入 JPEG 编码器。
pub fn encode_jpeg(rgb: &image::RgbImage, quality: u8) -> std::result::Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
        encoder
            .encode_image(rgb)
            .map_err(|e| format!("JPG 编码失败: {e}"))?;
    }
    if bytes.is_empty() {
        return Err("JPG 编码结果为空".to_string());
    }
    Ok(bytes)
}

/// 验证 JPG 确实可以正常打开：检查 SOI 标记并完整解码一次。
/// 原 C# 版用 `magick identify` 做这件事，这里改为纯 Rust。
pub fn verify_jpeg(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return false;
    }
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg) {
        Ok(img) => img.width() > 0 && img.height() > 0,
        Err(_) => false,
    }
}

fn fail(out: &mut ConvertOutcome, name: &str, reason: &str) {
    out.stats.failed += 1;
    out.logs.push(format!("✗ 失败: {name}  {reason}"));
}

/// 批量转换一个文件夹。返回统计与逐条日志，由调用方组装 ToolReport。
pub fn convert_directory(dir: &Path, opts: &ConvertOptions) -> Result<ConvertOutcome> {
    let mut out = ConvertOutcome::default();

    if !dir.is_dir() {
        return Err(ToolboxError::InvalidArgument(format!(
            "目标不是有效文件夹: {}",
            dir.display()
        )));
    }

    let files = collect_targets(dir, opts.recursive)?;
    out.stats.total = files.len();
    if files.is_empty() {
        out.logs.push("该目录没有 HEIC/HEIF 文件，无需转换。".to_string());
        return Ok(out);
    }

    let extra = [
        if opts.recursive { "，含子文件夹" } else { "" },
        if opts.delete_source {
            "，成功后删除原文件"
        } else {
            ""
        },
    ]
    .concat();
    out.logs.push(format!(
        "找到 {} 个 HEIC/HEIF 文件，质量 {}{}",
        out.stats.total, opts.quality, extra
    ));
    out.logs.push("-".repeat(46));

    let config = heic::DecoderConfig::new();

    for (index, src) in files.iter().enumerate() {
        let step = index + 1;
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| src.display().to_string());
        let dest = target_path(src);

        // 已有同名 JPG：跳过且不删除对应 HEIC。
        if should_skip(&dest) {
            out.stats.skipped += 1;
            out.logs.push(format!("· 跳过（同名 JPG 已存在）: {name}"));
            continue;
        }

        let src_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        let bytes = match std::fs::read(src) {
            Ok(b) => b,
            Err(e) => {
                fail(&mut out, &name, &format!("读取失败: {e}"));
                continue;
            }
        };

        let decoded = match decode_heic(&config, &bytes) {
            Ok(d) => d,
            Err(e) => {
                fail(&mut out, &name, &e);
                continue;
            }
        };
        if decoded.width == 0 || decoded.height == 0 || decoded.rgba.is_empty() {
            fail(&mut out, &name, "解码结果为空");
            continue;
        }

        let rgba = match image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba) {
            Some(img) => img,
            None => {
                fail(&mut out, &name, "解码像素数据长度异常");
                continue;
            }
        };

        let orientation = match config.extract_exif(&bytes) {
            Ok(Some(exif)) => orientation_from_exif(exif.as_ref()),
            _ => Orientation::NoTransforms,
        };

        let mut dynamic = image::DynamicImage::ImageRgba8(rgba);
        if orientation != Orientation::NoTransforms {
            dynamic.apply_orientation(orientation);
        }
        // JPEG 不支持 alpha，显式转 RGB；heic 已做 YCbCr→RGB（BT.601/709/2020），
        // 输出即 sRGB 数值，与原 C# 版 `-strip` 后的效果一致。
        let rgb = dynamic.to_rgb8();

        let jpeg = match encode_jpeg(&rgb, opts.quality) {
            Ok(b) => b,
            Err(e) => {
                fail(&mut out, &name, &e);
                continue;
            }
        };
        if !verify_jpeg(&jpeg) {
            fail(&mut out, &name, "内存中的 JPG 校验失败");
            continue;
        }

        if let Err(e) = std::fs::write(&dest, &jpeg) {
            fail(&mut out, &name, &format!("写入 JPG 失败: {e}"));
            continue;
        }

        // 删除原文件前，按原 C# 版的语义再校验一次「磁盘上的 JPG 可正常打开」。
        let disk_ok = std::fs::read(&dest)
            .map(|b| verify_jpeg(&b))
            .unwrap_or(false);
        if !disk_ok {
            let _ = std::fs::remove_file(&dest);
            fail(&mut out, &name, "输出校验失败，已删除损坏文件");
            continue;
        }

        out.stats.succeeded += 1;
        out.stats.bytes_out += jpeg.len() as u64;
        let kb = jpeg.len() / 1024;

        if opts.delete_source {
            match std::fs::remove_file(src) {
                Ok(()) => {
                    out.stats.bytes_freed += src_len;
                    out.logs.push(format!(
                        "[{step}/{}] {name}  →  {kb} KB  (原文件已删除)",
                        out.stats.total
                    ));
                }
                Err(e) => {
                    out.logs.push(format!(
                        "[! ] {name} 转换成功，但删除原文件失败: {e}"
                    ));
                }
            }
        } else {
            out.logs.push(format!("[{step}/{}] {name}  →  {kb} KB", out.stats.total));
        }
    }

    out.logs.push("-".repeat(46));
    out.logs.push(format!(
        "成功 {} · 跳过 {} · 失败 {} / 共 {}",
        out.stats.succeeded, out.stats.skipped, out.stats.failed, out.stats.total
    ));
    out.logs.push(format!(
        "JPG 体积 {:.2} MB，删除原文件释放 {:.2} MB",
        out.stats.bytes_out as f64 / 1048576.0,
        out.stats.bytes_freed as f64 / 1048576.0
    ));

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "tool_heic2jpg_{tag}_{nanos}_{}_{seq}",
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

    /// 用 image crate 现场生成一张小 JPEG，避免测试依赖二进制 fixture。
    fn sample_jpeg() -> Vec<u8> {
        let pixels: Vec<u8> = (0..(4usize * 3))
            .flat_map(|i| [i as u8, (i * 7) as u8, 128])
            .collect();
        let rgb = image::RgbImage::from_raw(4, 3, pixels).unwrap();
        encode_jpeg(&rgb, 90).unwrap()
    }

    /// 构造一个最小的 TIFF/IFD0，只包含 Orientation 标签。
    fn make_tiff(little: bool, orientation: u16, with_prefix: bool) -> Vec<u8> {
        let u16bytes = |n: u16| {
            if little {
                n.to_le_bytes().to_vec()
            } else {
                n.to_be_bytes().to_vec()
            }
        };
        let u32bytes = |n: u32| {
            if little {
                n.to_le_bytes().to_vec()
            } else {
                n.to_be_bytes().to_vec()
            }
        };

        let mut v = Vec::new();
        if with_prefix {
            v.extend_from_slice(b"Exif\0\0");
        }
        v.extend_from_slice(if little { b"II" } else { b"MM" });
        v.extend_from_slice(&u16bytes(42)); // magic
        v.extend_from_slice(&u32bytes(8)); // IFD0 offset
        v.extend_from_slice(&u16bytes(1)); // 1 个 entry
        v.extend_from_slice(&u16bytes(0x0112)); // Orientation
        v.extend_from_slice(&u16bytes(3)); // SHORT
        v.extend_from_slice(&u32bytes(1)); // count = 1
        v.extend_from_slice(&u16bytes(orientation)); // value
        v.extend_from_slice(&[0, 0]); // value 域补齐 4 字节
        v.extend_from_slice(&u32bytes(0)); // 没有下一个 IFD
        v
    }

    #[test]
    fn collect_targets_recursive_vs_top_level() {
        let tmp = TempDir::new("collect");
        fs::write(tmp.join("a.HEIC"), b"x").unwrap();
        fs::write(tmp.join("b.heif"), b"x").unwrap();
        fs::write(tmp.join("notes.txt"), b"x").unwrap();
        fs::create_dir_all(tmp.join("sub/deep")).unwrap();
        fs::write(tmp.join("sub/c.Heic"), b"x").unwrap();
        fs::write(tmp.join("sub/deep/d.heic"), b"x").unwrap();

        let top = collect_targets(tmp.path(), false).unwrap();
        let top_names: Vec<String> = top
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(top_names, vec!["a.HEIC", "b.heif"]);

        let all = collect_targets(tmp.path(), true).unwrap();
        assert_eq!(all.len(), 4);
        let all_names: Vec<String> = all
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(all_names.contains(&"c.Heic".to_string()));
        assert!(all_names.contains(&"d.heic".to_string()));
    }

    #[test]
    fn should_skip_only_nonempty_files() {
        let tmp = TempDir::new("skip");
        let jpg = tmp.join("empty.jpg");
        fs::write(&jpg, b"").unwrap();
        assert!(!should_skip(&jpg));
        fs::write(&jpg, b"nonempty").unwrap();
        assert!(should_skip(&jpg));
        assert!(!should_skip(&tmp.join("missing.jpg")));
        fs::create_dir(tmp.join("dir.jpg")).unwrap();
        assert!(!should_skip(&tmp.join("dir.jpg")));
    }

    #[test]
    fn build_options_defaults_and_overrides() {
        let defaults = build_options(&serde_json::json!({}));
        assert_eq!(defaults, ConvertOptions::default());
        assert_eq!(defaults.quality, 95);
        assert!(defaults.recursive);
        assert!(defaults.delete_source);

        let custom = build_options(&serde_json::json!({
            "quality": 60,
            "recursive": false,
            "delete_source": false
        }));
        assert_eq!(custom.quality, 60);
        assert!(!custom.recursive);
        assert!(!custom.delete_source);
    }

    #[test]
    fn build_options_clamps_quality() {
        assert_eq!(build_options(&serde_json::json!({ "quality": 0 })).quality, 1);
        assert_eq!(
            build_options(&serde_json::json!({ "quality": 150 })).quality,
            100
        );
        assert_eq!(
            build_options(&serde_json::json!({ "quality": -5 })).quality,
            1
        );
        assert_eq!(
            build_options(&serde_json::json!({ "quality": 33.6 })).quality,
            34
        );
        assert_eq!(
            build_options(&serde_json::json!({ "quality": "oops" })).quality,
            95
        );
    }

    #[test]
    fn parse_exif_orientation_little_and_big_endian() {
        assert_eq!(parse_exif_orientation(&make_tiff(true, 6, false)), Some(6));
        assert_eq!(parse_exif_orientation(&make_tiff(false, 8, false)), Some(8));
        assert_eq!(
            Orientation::from_exif(6),
            Some(Orientation::Rotate90)
        );
    }

    #[test]
    fn parse_exif_orientation_prefix_and_invalid_input() {
        assert_eq!(parse_exif_orientation(&make_tiff(true, 1, true)), Some(1));
        assert_eq!(parse_exif_orientation(&[]), None);
        assert_eq!(parse_exif_orientation(b"not a tiff at all"), None);
        assert_eq!(parse_exif_orientation(&make_tiff(true, 9, false)), None);
    }

    #[test]
    fn verify_jpeg_accepts_valid_and_rejects_garbage() {
        let valid = sample_jpeg();
        assert!(verify_jpeg(&valid));
        assert!(!verify_jpeg(&[]));
        assert!(!verify_jpeg(b"hello world"));
        assert!(!verify_jpeg(&valid[..10]));
    }

    #[test]
    fn target_path_keeps_same_directory() {
        assert_eq!(
            target_path(Path::new("D:/photos/a.b.HEIC")),
            PathBuf::from("D:/photos/a.b.jpg")
        );
        assert_eq!(
            target_path(Path::new("/tmp/photo.heif")),
            PathBuf::from("/tmp/photo.jpg")
        );
    }

    #[test]
    fn no_heic_returns_friendly_message() {
        let tmp = TempDir::new("empty");
        fs::write(tmp.join("readme.txt"), b"x").unwrap();
        let outcome = convert_directory(tmp.path(), &ConvertOptions::default()).unwrap();
        assert_eq!(outcome.stats.total, 0);
        assert!(outcome
            .logs
            .iter()
            .any(|line| line.contains("没有 HEIC/HEIF 文件")));
    }

    #[test]
    fn existing_jpg_is_skipped_and_source_is_kept() {
        let tmp = TempDir::new("skipconv");
        let heic = tmp.join("photo.heic");
        let jpg = tmp.join("photo.jpg");
        // 故意写入非法 HEIC 内容：若代码错误地尝试解码就会失败，
        // 从而验证「同名 JPG 存在时确实提前跳过」。
        fs::write(&heic, b"garbage-not-a-real-heic").unwrap();
        fs::write(&jpg, b"existing jpg bytes").unwrap();

        let opts = ConvertOptions {
            delete_source: true,
            ..Default::default()
        };
        let outcome = convert_directory(tmp.path(), &opts).unwrap();
        assert_eq!(outcome.stats.total, 1);
        assert_eq!(outcome.stats.skipped, 1);
        assert_eq!(outcome.stats.failed, 0);
        assert_eq!(outcome.stats.succeeded, 0);
        assert!(heic.exists());
        assert_eq!(fs::read(&jpg).unwrap().as_slice(), &b"existing jpg bytes"[..]);
    }
}
