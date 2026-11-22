//! 恢复 Windows 11 经典右键菜单。
//!
//! 背景：2026-11-12 的 KB5132061 后，旧的“单 CLSID”方案失效；
//! 据 Pureinfotech 2026-11-13 的报道，需同时写入两个 CLSID 的空
//! InprocServer32 项才能强制 Explorer 回退到经典菜单。此结论来自
//! 社区实测报道，**未在真机验证**，工具会在报告里明确提示。
//!
//! 安全边界：
//! - 只写 HKCU，不提权，不影响其他用户与系统文件
//! - 写入前备份被覆盖的值；关闭时整键删除并可在 GUI 一键切回
//! - 不随安装默认启用，必须用户显式确认

use serde::Serialize;
use toolbox_core::{Capabilities, Result, RunContext, Tool, ToolReport, ToolboxError};

#[cfg(windows)]
mod windows_impl;

pub const CLSID_LEGACY: &str = "{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";
pub const CLSID_NEW: &str = "{51668040-5e31-4023-bbe3-149163bd80f1}";

#[derive(Debug, Clone, Serialize)]
pub struct ClassicMenuStatus {
    pub enabled: bool,
    pub supported: bool,
    pub detail: String,
}

pub struct ClassicMenuTool;

impl Tool for ClassicMenuTool {
    fn id(&self) -> &'static str {
        "classic-menu"
    }

    fn name(&self) -> &'static str {
        "恢复 Windows 11 经典右键菜单"
    }

    fn description(&self) -> &'static str {
        "一键在新菜单与经典菜单间切换。只写当前用户注册表，无需管理员，可随时还原。"
    }

    fn platforms(&self) -> &'static [&'static str] {
        &["windows"]
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            context_menu: Vec::new(),
            system_tweak: true,
        }
    }

    fn run(&self, ctx: RunContext) -> Result<ToolReport> {
        let enabled = ctx
            .options
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| ToolboxError::InvalidArgument("缺少 enabled 布尔参数".into()))?;
        set_classic_menu(enabled)
    }
}

pub fn classic_menu_status() -> ClassicMenuStatus {
    #[cfg(windows)]
    {
        windows_impl::status()
    }
    #[cfg(not(windows))]
    {
        ClassicMenuStatus {
            enabled: false,
            supported: false,
            detail: "仅 Windows 支持".into(),
        }
    }
}

pub fn set_classic_menu(enabled: bool) -> Result<ToolReport> {
    #[cfg(windows)]
    {
        windows_impl::set(enabled)
    }
    #[cfg(not(windows))]
    {
        Err(ToolboxError::UnsupportedPlatform(
            "经典菜单只适用于 Windows".into(),
        ))
    }
}
