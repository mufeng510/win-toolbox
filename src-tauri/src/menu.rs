//! 右键菜单集中管理器：从 ToolRegistry 生成菜单项，全量同步注册表。
//! 菜单 id 形如 win-toolbox.hash-rename.directory.path_v，避免与其他软件冲突。

use serde::Serialize;
use toolbox_core::{MenuArg, MenuLocation, MenuVerb};

use crate::build_registry;

#[derive(Serialize, Clone)]
pub struct MenuStatusItem {
    pub id: String,
    pub tool_id: String,
    pub tool_name: Option<String>,
    pub location: MenuLocation,
    pub label: String,
    pub arg: MenuArg,
    pub enabled: bool,
}

struct VerbEntry {
    id: String,
    tool_id: String,
    tool_name: String,
    verb: MenuVerb,
}

fn loc_str(loc: MenuLocation) -> &'static str {
    match loc {
        MenuLocation::Directory => "directory",
        MenuLocation::DirectoryBackground => "directory_background",
        MenuLocation::File => "file",
    }
}

fn arg_str(arg: MenuArg) -> &'static str {
    match arg {
        MenuArg::PathV => "path_v",
        MenuArg::Path1 => "path1",
    }
}

pub fn verb_id(tool_id: &str, verb: &MenuVerb) -> String {
    format!("win-toolbox.{}.{}.{}", tool_id, loc_str(verb.location), arg_str(verb.arg))
}

fn entries() -> Vec<VerbEntry> {
    let registry = build_registry();
    let mut out = Vec::new();
    for tool in registry.available() {
        let tool_id = tool.id().to_string();
        let tool_name = tool.name().to_string();
        for verb in tool.capabilities().context_menu {
            out.push(VerbEntry {
                id: verb_id(&tool_id, &verb),
                tool_id: tool_id.clone(),
                tool_name: tool_name.clone(),
                verb,
            });
        }
    }
    out
}

fn all_ids() -> Vec<String> {
    entries().into_iter().map(|e| e.id).collect()
}

#[cfg(windows)]
fn key_base(loc: MenuLocation, id: &str) -> String {
    format!(r"{}\{}", loc.registry_suffix(), id)
}

#[cfg(windows)]
fn is_enabled(loc: MenuLocation, id: &str) -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(key_base(loc, id))
        .is_ok()
}

#[cfg(not(windows))]
fn is_enabled(_loc: MenuLocation, _id: &str) -> bool {
    false
}

#[cfg(windows)]
fn install_one(entry: &VerbEntry) -> Result<String, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let exe = std::env::current_exe()
        .map_err(|e| format!("无法确定程序路径: {e}"))?
        .display()
        .to_string();
    let base = key_base(entry.verb.location, &entry.id);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let (key, _) = hkcu
        .create_subkey(&base)
        .map_err(|e| format!("创建 HKCU\\{base} 失败: {e}"))?;
    key.set_value("", &entry.verb.label)
        .map_err(|e| format!("写入菜单名称失败: {e}"))?;
    key.set_value("Icon", &exe)
        .map_err(|e| format!("写入图标失败: {e}"))?;

    let cmd_path = format!(r"{base}\command");
    let (cmd, _) = hkcu
        .create_subkey(&cmd_path)
        .map_err(|e| format!("创建 HKCU\\{cmd_path} 失败: {e}"))?;
    cmd.set_value(
        "",
        &format!(
            "\"{exe}\" --tool {} --path {}",
            entry.tool_id,
            entry.verb.arg.as_str()
        ),
    )
    .map_err(|e| format!("写入命令失败: {e}"))?;

    Ok(format!("已安装: {}（{}）", entry.verb.label, loc_str(entry.verb.location)))
}

#[cfg(windows)]
fn uninstall_one(entry: &VerbEntry) -> Result<String, String> {
    delete_tree(&key_base(entry.verb.location, &entry.id));
    Ok(format!("已卸载: {}", entry.id))
}

/// 清理清单里已不存在的历史菜单项（防止工具移除后留下幽灵菜单）。
#[cfg(windows)]
fn uninstall_stale(id: &str) {
    for loc in [
        MenuLocation::Directory,
        MenuLocation::DirectoryBackground,
        MenuLocation::File,
    ] {
        delete_tree(&key_base(loc, id));
    }
}

#[cfg(windows)]
fn delete_tree(subkey: &str) {
    use windows_sys::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, wide.as_ptr());
    }
}

#[cfg(not(windows))]
fn install_one(_entry: &VerbEntry) -> Result<String, String> {
    Err("右键菜单管理目前仅支持 Windows".into())
}

#[cfg(not(windows))]
fn uninstall_one(_entry: &VerbEntry) -> Result<String, String> {
    Err("右键菜单管理目前仅支持 Windows".into())
}

#[cfg(not(windows))]
fn uninstall_stale(_id: &str) {}

#[tauri::command]
pub fn menu_status() -> Vec<MenuStatusItem> {
    entries()
        .into_iter()
        .map(|e| MenuStatusItem {
            id: e.id.clone(),
            tool_id: e.tool_id,
            tool_name: Some(e.tool_name),
            location: e.verb.location,
            label: e.verb.label,
            arg: e.verb.arg,
            enabled: is_enabled(e.verb.location, &e.id),
        })
        .collect()
}

#[tauri::command]
pub fn install_menus(ids: Vec<String>) -> Result<Vec<String>, String> {
    let all = entries();
    let mut report = Vec::new();
    for id in &ids {
        match all.iter().find(|e| &e.id == id) {
            Some(entry) => report.push(install_one(entry)?),
            None => return Err(format!("未知的菜单 id: {id}")),
        }
    }
    Ok(report)
}

#[tauri::command]
pub fn uninstall_menus(ids: Vec<String>) -> Result<Vec<String>, String> {
    let all = entries();
    let mut report = Vec::new();
    for id in &ids {
        match all.iter().find(|e| &e.id == id) {
            Some(entry) => report.push(uninstall_one(entry)?),
            None => {
                uninstall_stale(id);
                report.push(format!("已清理历史菜单项: {id}"));
            }
        }
    }
    Ok(report)
}

pub fn install_all() -> Result<Vec<String>, String> {
    install_menus(all_ids())
}

pub fn uninstall_all() -> Result<Vec<String>, String> {
    uninstall_menus(all_ids())
}
