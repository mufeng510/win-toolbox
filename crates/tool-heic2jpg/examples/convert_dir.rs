//! 开发辅助：对真实 HEIC 文件目录跑一遍转换核心，打印结果报告。
//!
//! 用法（Windows，自动配置 MSVC/SDK 环境）：
//!   powershell -ExecutionPolicy Bypass -File scripts\cargo.ps1 run -p tool-heic2jpg --example convert_dir -- <目录>

use std::path::PathBuf;

use tool_heic2jpg::HeicToJpgTool;
use toolbox_core::{RunContext, Tool};

fn main() {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: convert_dir <directory>"),
    );

    // 默认：质量 85、不递归、不删除原文件（安全，可重复运行）。
    let options = serde_json::json!({
        "quality": 85,
        "recursive": false,
        "delete_source": false,
    });

    let report = HeicToJpgTool::new()
        .run(RunContext {
            target: Some(dir),
            options,
        })
        .expect("转换失败");

    println!("OK={}", report.ok);
    println!("MESSAGE={}", report.message);
    for line in &report.details {
        println!("DETAIL: {}", line);
    }
}
