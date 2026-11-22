//! 单 exe 分流：无参数开 GUI，有参数走无界面调用（右键菜单入口）。

use std::path::PathBuf;

use toolbox_core::{config, Result, RunContext, ToolReport, ToolboxError};

use crate::{build_registry, commands, menu};

const HELP: &str = "win-toolbox\n\n\
  （无参数）              打开工具箱主界面\n\
  --tool <id> --path <p>  运行指定工具（右键菜单使用）\n\
  --install-menus         安装全部右键菜单\n\
  --uninstall-menus       卸载全部右键菜单\n\
  --help                  显示帮助";

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

pub fn headless(args: &[String]) -> Result<ToolReport> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(ToolReport::ok(HELP));
    }

    if args
        .iter()
        .any(|a| a == "--install-menus" || a == "--install-context-menu")
    {
        let details = menu::install_all().map_err(ToolboxError::System)?;
        return Ok(ToolReport {
            ok: true,
            message: "右键菜单已安装（当前用户，HKCU）".into(),
            details,
        });
    }

    if args
        .iter()
        .any(|a| a == "--uninstall-menus" || a == "--uninstall-context-menu")
    {
        let details = menu::uninstall_all().map_err(ToolboxError::System)?;
        return Ok(ToolReport {
            ok: true,
            message: "右键菜单已卸载（当前用户，HKCU）".into(),
            details,
        });
    }

    let tool_id = value_after(args, "--tool")
        .ok_or_else(|| ToolboxError::InvalidArgument("缺少 --tool 参数".into()))?;
    let target = value_after(args, "--path").map(PathBuf::from);

    let registry = build_registry();
    let tool = registry
        .get(&tool_id)
        .ok_or_else(|| ToolboxError::InvalidArgument(format!("未知工具: {tool_id}")))?;

    let section = config::tool_section(&tool_id)?;
    let options = commands::normalize_options(&tool_id, &section);

    tool.run(RunContext { target, options })
}

#[cfg(windows)]
pub fn show_message(title: &str, details: &[String], message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let mut text = message.to_string();
    for line in details {
        text.push_str("\r\n");
        text.push_str(line);
    }
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(windows))]
pub fn show_message(_title: &str, _details: &[String], _message: &str) {}
