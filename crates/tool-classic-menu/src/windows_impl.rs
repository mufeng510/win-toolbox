use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use toolbox_core::config::config_dir;
use toolbox_core::{Result, ToolReport, ToolboxError};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use crate::{ClassicMenuStatus, CLSID_LEGACY, CLSID_NEW};

const BASE: &str = r"Software\Classes\CLSID";

fn clsids() -> [&'static str; 2] {
    [CLSID_LEGACY, CLSID_NEW]
}

fn inproc_path(clsid: &str) -> String {
    format!(r"{BASE}\{clsid}\InprocServer32")
}

fn clsid_path(clsid: &str) -> String {
    format!(r"{BASE}\{clsid}")
}

fn backup_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("classic-menu-backup.json"))
}

pub fn status() -> ClassicMenuStatus {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let flags: Vec<bool> = clsids()
        .iter()
        .map(|c| hkcu.open_subkey(inproc_path(c)).is_ok())
        .collect();
    let enabled = flags.iter().all(|f| *f);
    let detail = if enabled {
        "两个 CLSID 的空 InprocServer32 均已写入，经典菜单应已生效。".into()
    } else if flags.iter().any(|f| *f) {
        "只有部分 CLSID 已写入，状态不完整。建议点“启用”重新写入。".into()
    } else {
        "未启用，当前为 Windows 11 新菜单。".into()
    };
    ClassicMenuStatus {
        enabled,
        supported: true,
        detail,
    }
}

/// 写入/删除两个 CLSID，并重启资源管理器使其生效。
pub fn set(enabled: bool) -> Result<ToolReport> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut details = Vec::new();

    if enabled {
        // 覆盖前备份原值，保证可逆。
        let mut backup: HashMap<String, String> = HashMap::new();
        for clsid in clsids() {
            let path = inproc_path(clsid);
            let existing = match hkcu.open_subkey(&path) {
                Ok(k) => k.get_value::<String, _>("").unwrap_or_default(),
                Err(_) => String::new(),
            };
            backup.insert(clsid.to_string(), existing);

            let (key, _) = hkcu
                .create_subkey(&path)
                .map_err(|e| ToolboxError::System(format!("创建 {path} 失败: {e}")))?;
            // 空字符串默认值：让 Explorer 加载不到新版菜单 COM 组件，回退经典菜单。
            key.set_value("", &"")
                .map_err(|e| ToolboxError::System(format!("写入 {path} 默认值失败: {e}")))?;
            details.push(format!("已写入 HKCU\\{path} = (空)"));
        }

        let path = backup_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&backup)?)
            .map_err(|e| ToolboxError::System(format!("写入备份失败: {e}")))?;
        details.push(format!("原值已备份到 {}", path.display()));
    } else {
        for clsid in clsids() {
            delete_tree(&clsid_path(clsid));
            details.push(format!("已删除 HKCU\\{}", clsid_path(clsid)));
        }
        details.push("如需恢复旧值，请查看 classic-menu-backup.json。".into());
    }

    restart_explorer(&mut details);

    Ok(ToolReport {
        ok: true,
        message: if enabled {
            "已启用经典右键菜单（当前用户），资源管理器已重启。".into()
        } else {
            "已恢复 Windows 11 新菜单（当前用户），资源管理器已重启。".into()
        },
        details,
    })
}

/// RegDeleteTreeW：删键及全部子键；键不存在时静默忽略（幂等）。
fn delete_tree(subkey: &str) {
    use windows_sys::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, wide.as_ptr());
    }
}

fn restart_explorer(details: &mut Vec<String>) {
    let killed = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if killed {
        std::thread::sleep(Duration::from_millis(800));
        let restarted = Command::new("explorer.exe")
            .spawn()
            .map(|_| true)
            .unwrap_or(false);
        details.push(if restarted {
            "资源管理器已自动重启。".into()
        } else {
            "资源管理器已结束，请手动启动 explorer.exe。".into()
        });
    } else {
        details.push("未能自动结束 explorer.exe，请注销重登或手动重启资源管理器后生效。".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_well_formed() {
        assert!(inproc_path(CLSID_LEGACY).ends_with(r"\InprocServer32"));
        assert!(clsid_path(CLSID_NEW).starts_with(r"Software\Classes\CLSID\{"));
    }
}
