use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use toolbox_core::{
    config, current_platform, Capabilities, RunContext, ToolReport,
};

use crate::{build_registry, manifest};

#[derive(Serialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub platforms: Vec<String>,
    pub capabilities: Capabilities,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
}

/// 把前端配置键名映射到各工具 crate 期望的键名。
/// 前端：algorithm/preview/delete_original；核心：hash/dry_run/delete_source。
pub fn normalize_options(tool_id: &str, options: &Value) -> Value {
    let mut out = options
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);

    match tool_id {
        "hash-rename" => {
            let hash = options
                .get("hash")
                .or_else(|| options.get("algorithm"))
                .cloned()
                .unwrap_or_else(|| Value::String("md5".into()));
            out.insert("hash".into(), hash);
            let dry = options
                .get("dry_run")
                .or_else(|| options.get("preview"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            out.insert("dry_run".into(), Value::Bool(dry));
        }
        "heic-to-jpg" => {
            let del = options
                .get("delete_source")
                .or_else(|| options.get("delete_original"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            out.insert("delete_source".into(), Value::Bool(del));
        }
        _ => {}
    }

    Value::Object(out)
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: current_platform().to_string(),
    }
}

#[tauri::command]
pub fn list_tools() -> Vec<ToolInfo> {
    let platform = current_platform();
    manifest()
        .tools
        .into_iter()
        .filter(|t| t.platforms.is_empty() || t.platforms.iter().any(|p| p == platform))
        .map(|t| ToolInfo {
            id: t.id,
            name: t.name,
            description: t.description,
            platforms: t.platforms,
            capabilities: t.capabilities,
        })
        .collect()
}

#[tauri::command]
pub fn run_tool(id: String, target: Option<String>, options: Value) -> ToolReport {
    let registry = build_registry();
    let Some(tool) = registry.get(&id) else {
        return ToolReport::fail(format!("未知工具: {id}"));
    };

    let target = target.filter(|s| !s.trim().is_empty()).map(PathBuf::from);
    if target.is_none() && id != "classic-menu" {
        return ToolReport::fail("请先选择或输入目标文件夹。");
    }

    let options = normalize_options(&id, &options);
    tool.run(RunContext { target, options })
        .unwrap_or_else(|e| ToolReport::fail(e.to_string()))
}

#[tauri::command]
pub fn classic_menu_status() -> tool_classic_menu::ClassicMenuStatus {
    tool_classic_menu::classic_menu_status()
}

#[tauri::command]
pub fn classic_menu_set(enabled: bool) -> ToolReport {
    tool_classic_menu::set_classic_menu(enabled)
        .unwrap_or_else(|e| ToolReport::fail(e.to_string()))
}

#[tauri::command]
pub fn get_config() -> Result<Value, String> {
    config::load().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_config(config: Value) -> Result<(), String> {
    // 参数名保持 config 以匹配前端 invoke({ config })，用完整路径避免遮蔽模块。
    toolbox_core::config::save(&config).map_err(|e| e.to_string())
}
