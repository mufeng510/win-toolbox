//! 统一配置：%APPDATA%\win-toolbox\config.json（Unix 为 ~/.config/win-toolbox）。

use std::path::PathBuf;

use serde_json::Value;

use crate::error::{Result, ToolboxError};

pub fn config_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"));

    let base = base.ok_or_else(|| ToolboxError::Config("无法定位配置目录".into()))?;
    Ok(base.join("win-toolbox"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn load() -> Result<Value> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    let text = std::fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(&text).map_err(Into::into)
}

pub fn save(value: &Value) -> Result<()> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn tool_section(tool_id: &str) -> Result<Value> {
    let root = load()?;
    Ok(root.get(tool_id).cloned().unwrap_or(Value::Object(serde_json::Map::new())))
}

pub fn set_tool_section(tool_id: &str, section: Value) -> Result<()> {
    let mut root = load()?;
    root.as_object_mut()
        .ok_or_else(|| ToolboxError::Config("config.json 顶层必须是对象".into()))?
        .insert(tool_id.to_string(), section);
    save(&root)
}
