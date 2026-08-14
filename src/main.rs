#![cfg_attr(windows, windows_subsystem = "windows")]

mod cdp;
mod windows_app;

#[cfg(windows)]
fn main() {
    windows_app::reset_log();
    windows_app::append_log("launcher started");
    if let Err(error) = run() {
        windows_app::append_log(&format!("launcher failed: {error}"));
        windows_app::show_error(&format!(
            "{error}\n\n已尝试关闭本次启动留下的 ChatGPT 进程。请改用官方快捷方式启动。\n诊断日志：{}\n本程序不会修改或删除用户数据。",
            windows_app::log_path().display()
        ));
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("ITOC ChatGPT 中文启动器目前只支持 Windows。");
    std::process::exit(1);
}

#[cfg(windows)]
fn run() -> Result<(), String> {
    use std::net::TcpListener;

    let app = windows_app::detect()?;
    windows_app::append_log(&format!(
        "detected package {} version {}",
        app.package_full_name, app.version
    ));

    if std::env::args().any(|argument| argument == "--diagnose") {
        windows_app::show_info(&format!(
            "已检测到官方应用版本：{}\n官方包：{}\n\n检测完成；未启动应用，也未开放调试端口。",
            app.version, app.package_full_name
        ));
        return Ok(());
    }

    windows_app::ensure_not_running(&app)?;

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("无法选择本机临时调试端口：{error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("无法读取本机临时调试端口：{error}"))?
        .port();
    drop(listener);

    let arguments = format!(
        "--lang=zh-CN --remote-debugging-address=127.0.0.1 \
         --remote-debugging-port={port} \
         --remote-allow-origins=http://127.0.0.1:{port}"
    )
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");

    let process_id = windows_app::launch(&app.app_user_model_id, &arguments)?;
    windows_app::append_log(&format!(
        "activation requested: pid={process_id}, debug_port={port}"
    ));
    if let Err(error) = cdp::wait_and_inject(port) {
        windows_app::append_log(&format!("runtime integration failed: {error}"));
        if let Err(cleanup_error) = windows_app::stop_package_processes(&app) {
            windows_app::append_log(&format!("process cleanup failed: {cleanup_error}"));
            return Err(format!("{error}\n清理后台进程失败：{cleanup_error}"));
        }
        windows_app::append_log("package processes stopped after startup failure");
        return Err(error);
    }
    Ok(())
}
