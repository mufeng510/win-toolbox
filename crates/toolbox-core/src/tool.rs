use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// 右键菜单的注册位置。三个工具目前覆盖前两种，未来可加 File。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuLocation {
    /// 右键文件夹本身：HKCU\Software\Classes\Directory\shell
    Directory,
    /// 文件夹空白处右键：HKCU\Software\Classes\Directory\Background\shell
    DirectoryBackground,
    /// 右键文件：HKCU\Software\Classes\*\shell
    File,
}

impl MenuLocation {
    pub fn registry_suffix(self) -> &'static str {
        match self {
            Self::Directory => r"Software\Classes\Directory\shell",
            Self::DirectoryBackground => r"Software\Classes\Directory\Background\shell",
            Self::File => r"Software\Classes\*\shell",
        }
    }
}

/// 传给 exe 的路径参数形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuArg {
    /// %V：资源管理器传文件夹路径（UNC 也可）
    PathV,
    /// %1：右键项目本身的路径
    Path1,
}

impl MenuArg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PathV => "%V",
            Self::Path1 => "%1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuVerb {
    pub location: MenuLocation,
    pub label: String,
    pub arg: MenuArg,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub context_menu: Vec<MenuVerb>,
    /// 系统设置类工具：默认不随安装启用，需用户在 GUI 显式确认。
    #[serde(default)]
    pub system_tweak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunContext {
    /// 右键/命令行传入的目标路径；从首页启动时为 None。
    pub target: Option<PathBuf>,
    /// 工具自身配置（来自统一 config.json 的对应 section）。
    #[serde(default)]
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReport {
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub details: Vec<String>,
}

impl ToolReport {
    pub fn ok(message: impl Into<String>) -> Self {
        Self { ok: true, message: message.into(), details: Vec::new() }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self { ok: false, message: message.into(), details: Vec::new() }
    }
}

/// 每个工具实现这个 trait；registry 按 id 分发。
pub trait Tool: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// 空数组表示全平台；否则只在这些平台显示/可用。
    fn platforms(&self) -> &'static [&'static str];
    fn capabilities(&self) -> Capabilities;
    fn run(&self, ctx: RunContext) -> Result<ToolReport>;
}

/// 工具注册表：单 exe 分流的唯一入口。
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.id() == id).map(|t| t.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }

    /// 按当前平台过滤；Windows 为主，其他平台先隐藏 Windows-only 工具。
    pub fn available(&self) -> Vec<&dyn Tool> {
        let current = current_platform();
        self.tools
            .iter()
            .filter(|t| t.platforms().is_empty() || t.platforms().contains(&current))
            .map(|t| t.as_ref())
            .collect()
    }
}

pub fn current_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    { "windows" }
    #[cfg(target_os = "macos")]
    { "macos" }
    #[cfg(target_os = "linux")]
    { "linux" }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { "unknown" }
}
