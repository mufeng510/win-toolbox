//! 图片上下合并的核心逻辑。
//!
//! 这里放可单测的纯函数（自然排序、配对、格式选择、拼接、去重命名），
//! 以及单对图片的安全落盘流程（临时文件 → 验证 → 重命名 → 删原图）。

use std::cmp::Ordering;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbaImage};

use toolbox_core::{Result, ToolboxError};

/// 输出格式。两张源图都是 JPEG 时输出 JPG，否则输出 PNG。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Png,
}

impl OutputFormat {
    /// 输出文件扩展名（不含点）。
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::Png => "png",
        }
    }
}

/// 输出格式选择：仅当两张都是 JPEG 时用 JPG（可套用 quality），其余一律 PNG。
pub fn choose_format(first_is_jpeg: bool, second_is_jpeg: bool) -> OutputFormat {
    if first_is_jpeg && second_is_jpeg {
        OutputFormat::Jpeg
    } else {
        OutputFormat::Png
    }
}

/// 按扩展名判断源图是否为 JPEG（不读文件内容，避免为格式判断额外解码一次）。
pub fn is_jpeg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false)
}

/// 字符串自然排序：把连续数字当作数值比较，`img2` 排在 `img10` 前。
/// 数值相等时位数少者在前（`img2` 排在 `img02` 前）。
pub fn natural_cmp_str(a: &str, b: &str) -> Ordering {
    let mut ac = a.chars().peekable();
    let mut bc = b.chars().peekable();

    loop {
        match (ac.peek().copied(), bc.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    // 两边都是数字：取出各自完整的数字串，按数值比较。
                    let mut na: u128 = 0;
                    let mut len_a = 0usize;
                    while let Some(&d) = ac.peek() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        ac.next();
                        na = na
                            .saturating_mul(10)
                            .saturating_add((d as u8 - b'0') as u128);
                        len_a += 1;
                    }

                    let mut nb: u128 = 0;
                    let mut len_b = 0usize;
                    while let Some(&d) = bc.peek() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        bc.next();
                        nb = nb
                            .saturating_mul(10)
                            .saturating_add((d as u8 - b'0') as u128);
                        len_b += 1;
                    }

                    match na.cmp(&nb) {
                        Ordering::Equal => {
                            // 数值相同再比位数，保证顺序确定。
                            let by_len = len_a.cmp(&len_b);
                            if by_len != Ordering::Equal {
                                return by_len;
                            }
                        }
                        other => return other,
                    }
                } else {
                    ac.next();
                    bc.next();
                    let by_char = ca.cmp(&cb);
                    if by_char != Ordering::Equal {
                        return by_char;
                    }
                }
            }
        }
    }
}

/// 按「文件名」自然排序比较两个路径（不比较所在目录）。
pub fn natural_cmp(a: &Path, b: &Path) -> Ordering {
    let name_a = a
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name_b = b
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    natural_cmp_str(&name_a, &name_b)
}

/// 两两配对的结果。`skipped` 是奇数张时最后落单的那一张。
#[derive(Debug, Default)]
pub struct Pairing {
    pub pairs: Vec<(PathBuf, PathBuf)>,
    pub skipped: Vec<PathBuf>,
}

/// 先按文件名自然排序，再两两配对：1+2、3+4……落单的最后一张进入 skipped。
pub fn pair_paths(mut paths: Vec<PathBuf>) -> Pairing {
    // sort_by 是稳定排序；文件名相同的两项保持输入顺序。
    paths.sort_by(|a, b| natural_cmp(a, b));

    let mut result = Pairing::default();
    let mut iter = paths.into_iter();
    while let Some(first) = iter.next() {
        match iter.next() {
            Some(second) => result.pairs.push((first, second)),
            None => result.skipped.push(first),
        }
    }
    result
}

/// 生成同目录下的唯一输出路径；已存在时依次追加 " (1)"、" (2)"……绝不覆盖。
pub fn unique_output_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let candidate = dir.join(format!("{stem}.{ext}"));
    if !candidate.exists() {
        return candidate;
    }

    let mut index = 1usize;
    loop {
        let candidate = dir.join(format!("{stem} ({index}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

/// 进程内递增序号，配合时间戳避免临时文件名冲突。
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 生成同目录下的隐藏临时文件名（扩展名固定为 .tmp，不参与格式推断）。
pub fn temp_output_path(dir: &Path) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!(
        ".imgmerge-{}-{nanos}-{seq:x}.tmp",
        std::process::id()
    ))
}

/// 垂直拼接两张已解码的图片。
///
/// - 宽度必须完全相同，否则返回错误（调用方据此跳过这一对并继续处理其它对）。
/// - 画布高 `h = h1 + h2 - overlap`；第一张放 `0..h1`。
/// - 第二张从第 `overlap` 行开始复制，放到 `h1..h1+h2-overlap`，即裁掉第二张的顶部重叠行。
pub fn merge_images(
    first: &DynamicImage,
    second: &DynamicImage,
    overlap: u32,
) -> Result<RgbaImage> {
    let width = first.width();
    if second.width() != width {
        return Err(ToolboxError::Tool(format!(
            "两张图片宽度不同：{} vs {}，无法上下拼接",
            width,
            second.width()
        )));
    }

    let h1 = first.height();
    let h2 = second.height();
    if overlap >= h2 {
        return Err(ToolboxError::InvalidArgument(format!(
            "重叠行数 {overlap} 必须小于第二张图片高度 {h2}"
        )));
    }

    let height = merged_height(h1, h2, overlap)?;
    let mut canvas = RgbaImage::new(width, height);
    let first_rgba = first.to_rgba8();
    let second_rgba = second.to_rgba8();

    for y in 0..h1 {
        for x in 0..width {
            canvas.put_pixel(x, y, *first_rgba.get_pixel(x, y));
        }
    }
    for y in 0..(h2 - overlap) {
        for x in 0..width {
            canvas.put_pixel(x, h1 + y, *second_rgba.get_pixel(x, y + overlap));
        }
    }

    Ok(canvas)
}

fn merged_height(h1: u32, h2: u32, overlap: u32) -> Result<u32> {
    h1.checked_add(h2)
        .and_then(|sum| sum.checked_sub(overlap))
        .ok_or_else(|| ToolboxError::Tool("拼接后高度溢出".to_string()))
}

/// 单对图片的处理结果。任一步失败都会写入 `error`，且绝不删除该对原图
/// （删除只在全部前置步骤成功之后进行）。
#[derive(Debug)]
pub struct PairOutcome {
    pub index: usize,
    pub first: PathBuf,
    pub second: PathBuf,
    /// 成功落盘的输出文件（重命名后）。
    pub output: Option<PathBuf>,
    /// 已成功删除的原图列表。
    pub deleted: Vec<PathBuf>,
    /// 失败或警告信息。
    pub error: Option<String>,
}

fn append_error(slot: &mut Option<String>, message: String) {
    match slot {
        Some(existing) => {
            existing.push('；');
            existing.push_str(&message);
        }
        None => *slot = Some(message),
    }
}

/// 把画布按格式写入同目录的临时文件，返回临时文件路径。
fn write_temp(
    img: &RgbaImage,
    fmt: OutputFormat,
    quality: u8,
    dir: &Path,
) -> std::result::Result<PathBuf, String> {
    for _ in 0..8 {
        let temp = temp_output_path(dir);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("创建临时文件 {} 失败：{e}", temp.display())),
        };

        let encoded: image::ImageResult<()> = match fmt {
            OutputFormat::Jpeg => {
                // to_rgb8 是 DynamicImage 的方法，ImageBuffer 需要先包一层。
                let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
                let mut encoder = JpegEncoder::new_with_quality(&mut file, quality);
                encoder.encode_image(&rgb)
            }
            OutputFormat::Png => PngEncoder::new(&mut file).write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            ),
        };

        if let Err(e) = encoded {
            let _ = fs::remove_file(&temp);
            return Err(format!("编码临时文件 {} 失败：{e}", temp.display()));
        }
        if let Err(e) = file.flush() {
            let _ = fs::remove_file(&temp);
            return Err(format!("刷新临时文件 {} 失败：{e}", temp.display()));
        }
        if let Err(e) = file.sync_all() {
            let _ = fs::remove_file(&temp);
            return Err(format!("同步临时文件 {} 失败：{e}", temp.display()));
        }
        return Ok(temp);
    }

    Err("无法生成唯一的临时文件名".to_string())
}

/// 处理一对图片：解码 → 校验宽度并拼接 → 写临时文件 → 验证 → 重命名 → （可选）删原图。
///
/// 本函数不会因为单对失败而中断整批任务：失败信息写进返回的 `PairOutcome.error`。
pub fn merge_pair_files(
    first: &Path,
    second: &Path,
    overlap: u32,
    quality: u8,
    delete_sources: bool,
    index: usize,
) -> PairOutcome {
    let mut outcome = PairOutcome {
        index,
        first: first.to_path_buf(),
        second: second.to_path_buf(),
        output: None,
        deleted: Vec::new(),
        error: None,
    };

    let img_first = match image::open(first) {
        Ok(image) => image,
        Err(e) => {
            outcome.error = Some(format!("打开原图 {} 失败：{e}", first.display()));
            return outcome;
        }
    };
    let img_second = match image::open(second) {
        Ok(image) => image,
        Err(e) => {
            outcome.error = Some(format!("打开原图 {} 失败：{e}", second.display()));
            return outcome;
        }
    };

    let merged = match merge_images(&img_first, &img_second, overlap) {
        Ok(image) => image,
        Err(e) => {
            outcome.error = Some(e.to_string());
            return outcome;
        }
    };

    let format = choose_format(is_jpeg_path(first), is_jpeg_path(second));
    let dir = first
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = match first.file_stem() {
        Some(stem) => format!("{}_合并", stem.to_string_lossy()),
        None => {
            outcome.error = Some(format!("原图 {} 没有可用的文件名", first.display()));
            return outcome;
        }
    };
    let final_path = unique_output_path(dir, &stem, format.extension());

    // 关键安全流程：先写临时文件，验证通过后才重命名，最后才删原图。
    let temp = match write_temp(&merged, format, quality, dir) {
        Ok(path) => path,
        Err(e) => {
            outcome.error = Some(e);
            return outcome;
        }
    };

    // 临时文件故意用 .tmp 后缀（避免被误认成最终输出），
    // 所以验证必须按文件内容探测格式，不能用 image::open（它依赖扩展名）。
    let decoded = (|| -> std::result::Result<DynamicImage, String> {
        let reader = image::ImageReader::open(&temp)
            .map_err(|e| format!("打开临时文件失败：{e}"))?;
        let reader = reader
            .with_guessed_format()
            .map_err(|e| format!("探测临时文件格式失败：{e}"))?;
        reader.decode().map_err(|e| format!("解码失败：{e}"))
    })();

    let decoded = match decoded {
        Ok(image) => image,
        Err(e) => {
            let _ = fs::remove_file(&temp);
            outcome.error = Some(format!("临时文件 {} 验证失败：{e}", temp.display()));
            return outcome;
        }
    };
    if decoded.width() != merged.width() || decoded.height() != merged.height() {
        let _ = fs::remove_file(&temp);
        outcome.error = Some(format!(
            "临时文件尺寸校验失败：{}x{}，期望 {}x{}",
            decoded.width(),
            decoded.height(),
            merged.width(),
            merged.height()
        ));
        return outcome;
    }

    if let Err(e) = fs::rename(&temp, &final_path) {
        let _ = fs::remove_file(&temp);
        outcome.error = Some(format!(
            "重命名 {} → {} 失败：{e}",
            temp.display(),
            final_path.display()
        ));
        return outcome;
    }
    outcome.output = Some(final_path);

    if delete_sources {
        for source in [first, second] {
            match fs::remove_file(source) {
                Ok(()) => outcome.deleted.push(source.to_path_buf()),
                Err(e) => append_error(
                    &mut outcome.error,
                    format!("删除原图 {} 失败：{e}", source.display()),
                ),
            }
        }
    }

    outcome
}

/// 批量处理：先按文件名自然排序并配对，再逐对执行安全合并流程。
/// 返回（每对结果, 奇数时落单未处理的路径）。
pub fn merge_all(
    paths: Vec<PathBuf>,
    overlap: u32,
    quality: u8,
    delete_sources: bool,
) -> (Vec<PairOutcome>, Vec<PathBuf>) {
    let pairing = pair_paths(paths);
    let outcomes = pairing
        .pairs
        .iter()
        .enumerate()
        .map(|(index, (first, second))| {
            merge_pair_files(first, second, overlap, quality, delete_sources, index + 1)
        })
        .collect();
    (outcomes, pairing.skipped)
}
