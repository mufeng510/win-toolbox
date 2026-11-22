//! 工具清单：右键菜单安装/卸载的唯一事实来源。
//! 运行时嵌入（include_str!），安装/卸载按清单全量同步，避免幽灵菜单。

use serde::{Deserialize, Serialize};

use crate::tool::Capabilities;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub tools: Vec<ToolManifest>,
}

impl Manifest {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn menu_verbs(&self) -> Vec<(String, crate::tool::MenuVerb)> {
        self.tools
            .iter()
            .flat_map(|t| {
                t.capabilities
                    .context_menu
                    .iter()
                    .map(move |v| (t.id.clone(), v.clone()))
            })
            .collect()
    }
}
