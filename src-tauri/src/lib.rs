pub mod commands;
pub mod dispatch;
pub mod menu;

use toolbox_core::ToolRegistry;

/// 构建工具注册表。每个工具在 Tool trait 中声明自己的元数据与能力，
/// 这里是工具列表与右键菜单的唯一事实来源；每个命令调用都新建，无共享可变状态。
pub fn build_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(tool_hashrename::HashRenameTool));
    registry.register(Box::new(tool_heic2jpg::HeicToJpgTool::new()));
    registry.register(Box::new(tool_classic_menu::ClassicMenuTool));
    registry.register(Box::new(tool_flatten::FlattenTool));
    registry.register(Box::new(tool_image_merge::ImageMergeTool));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表完整性：新增工具后必须在这里登记，否则 list_tools / 右键菜单都看不到它。
    #[test]
    fn all_tools_are_registered() {
        let registry = build_registry();
        let ids: Vec<&str> = registry.all().iter().map(|t| t.id()).collect();
        assert_eq!(
            ids,
            vec![
                "hash-rename",
                "heic-to-jpg",
                "classic-menu",
                "folder-flatten",
                "image-merge",
            ]
        );
    }

    /// 右键菜单能力属于工具自身，不能像这次一样在重构中丢失。
    #[test]
    fn menu_verbs_are_declared() {
        let registry = build_registry();
        let expected = [
            ("hash-rename", 2),
            ("heic-to-jpg", 1),
            ("classic-menu", 0),
            ("folder-flatten", 2),
            ("image-merge", 0),
        ];
        for (id, count) in expected {
            let tool = registry.get(id).expect("工具未注册");
            assert_eq!(
                tool.capabilities().context_menu.len(),
                count,
                "{id} 的右键菜单动词数量不对"
            );
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_tools,
            menu::menu_status,
            menu::install_menus,
            menu::uninstall_menus,
            commands::run_tool,
            commands::classic_menu_status,
            commands::classic_menu_set,
            commands::get_config,
            commands::set_config,
        ])
        .run(tauri::generate_context!())
        .expect("启动 win-toolbox 失败");
}
