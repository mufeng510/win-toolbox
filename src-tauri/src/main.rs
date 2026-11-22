#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 无参数 → 打开 GUI；有参数 → 右键菜单/CLI 的无界面调用。
    if args.is_empty() {
        win_toolbox_lib::run();
        return;
    }

    // --quiet / -q：不弹消息框，结果只写到 stdout/stderr，便于脚本调用与自动化测试。
    // GUI 与右键菜单入口不带此参数，仍保留原有的中文消息框。
    let quiet = args.iter().any(|a| a == "--quiet" || a == "-q");

    match win_toolbox_lib::dispatch::headless(&args) {
        Ok(report) => {
            println!("OK={}", report.ok);
            println!("MESSAGE={}", report.message);
            for line in &report.details {
                println!("DETAIL: {}", line);
            }
            if !quiet {
                #[cfg(windows)]
                win_toolbox_lib::dispatch::show_message(
                    if report.ok { "win-toolbox" } else { "win-toolbox · 注意" },
                    &report.details,
                    &report.message,
                );
            }
            if !report.ok {
                std::process::exit(1);
            }
        }
        Err(e) => {
            let msg = e.to_string();
            eprintln!("ERROR: {}", msg);
            if !quiet {
                #[cfg(windows)]
                win_toolbox_lib::dispatch::show_message("win-toolbox · 错误", &[msg.clone()], &msg);
            }
            std::process::exit(2);
        }
    }
}

/// 预留：把 %V / %1 转成路径（当前由 dispatch 直接处理，保留此函数便于扩展）。
#[allow(dead_code)]
fn arg_to_path(value: Option<String>) -> Option<PathBuf> {
    value.filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}
