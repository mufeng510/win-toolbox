//! 批量 HEIC 转 JPG 工具 crate。
//!
//! 用纯 Rust 解码 HEIC/HEIF（imazen/heic），不再依赖 ImageMagick。
//! 实现 toolbox-core 的 `Tool` trait，由主程序按 id 分发。

pub mod convert;

use toolbox_core::{
    Capabilities, MenuArg, MenuLocation, MenuVerb, Result, RunContext, Tool, ToolReport,
    ToolboxError,
};

use crate::convert::ConvertOptions;

#[derive(Debug, Default, Clone, Copy)]
pub struct HeicToJpgTool;

impl HeicToJpgTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for HeicToJpgTool {
    fn id(&self) -> &'static str {
        "heic-to-jpg"
    }

    fn name(&self) -> &'static str {
        "批量 HEIC 转 JPG"
    }

    fn description(&self) -> &'static str {
        "把文件夹内的 HEIC/HEIF 转为 JPG，可按 EXIF 校正方向、调整质量、可选递归与删除原文件。纯 Rust 解码，无外部依赖。"
    }

    /// 空数组表示全平台；这里显式列出三个平台。
    fn platforms(&self) -> &'static [&'static str] {
        &["windows", "macos", "linux"]
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            context_menu: vec![MenuVerb {
                location: MenuLocation::DirectoryBackground,
                label: self.name().to_string(),
                arg: MenuArg::PathV,
            }],
            system_tweak: false,
        }
    }

    fn run(&self, ctx: RunContext) -> Result<ToolReport> {
        let target = ctx
            .target
            .ok_or_else(|| ToolboxError::InvalidArgument("未指定要转换的文件夹".to_string()))?;

        let opts: ConvertOptions = convert::build_options(&ctx.options);
        let outcome = convert::convert_directory(&target, &opts)?;
        let stats = outcome.stats;

        let message = if stats.total == 0 {
            "该目录没有 HEIC/HEIF 文件，无需转换。".to_string()
        } else {
            format!(
                "成功 {} · 跳过 {} · 失败 {} / 共 {}",
                stats.succeeded, stats.skipped, stats.failed, stats.total
            )
        };

        // 与原 C# 版退出码语义一致：有失败项即视为未完全成功。
        let mut report = if stats.failed == 0 {
            ToolReport::ok(message)
        } else {
            ToolReport::fail(message)
        };
        report.details = outcome.logs;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_matches_spec() {
        let tool = HeicToJpgTool::new();
        assert_eq!(tool.id(), "heic-to-jpg");
        assert_eq!(tool.platforms(), &["windows", "macos", "linux"][..]);
        let caps = tool.capabilities();
        assert!(!caps.system_tweak);
        assert_eq!(caps.context_menu.len(), 1);
        assert_eq!(caps.context_menu[0].location, MenuLocation::DirectoryBackground);
        assert_eq!(caps.context_menu[0].arg, MenuArg::PathV);
    }

    #[test]
    fn missing_target_returns_invalid_argument() {
        let ctx = RunContext {
            target: None,
            options: serde_json::Value::Null,
        };
        let err = HeicToJpgTool::new().run(ctx).unwrap_err();
        assert!(matches!(err, ToolboxError::InvalidArgument(_)));
    }
}
