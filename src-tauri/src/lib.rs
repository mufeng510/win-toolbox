pub mod commands;
pub mod dispatch;
pub mod menu;

use toolbox_core::{Manifest, ToolRegistry};

/// 工具清单：右键菜单注册与工具列表的唯一事实来源（编译期嵌入）。
pub fn manifest() -> Manifest {
    Manifest::from_json(include_str!("../../tools/manifest.json"))
        .expect("tools/manifest.json 必须是合法的 JSON")
}

/// 构建工具注册表。每个命令调用都新建，无共享可变状态。
pub fn build_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(tool_hashrename::HashRenameTool));
    registry.register(Box::new(tool_heic2jpg::HeicToJpgTool::new()));
    registry.register(Box::new(tool_classic_menu::ClassicMenuTool));
    registry.register(Box::new(tool_flatten::FlattenTool));
    registry.register(Box::new(tool_image_merge::ImageMergeTool));
    registry
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
