#![cfg_attr(windows, windows_subsystem = "windows")]

mod cdp;
mod windows_app;

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        windows_app::show_error(&format!(
            "{error}\n\n请改用官方 ChatGPT 快捷方式启动；本程序不会修改或删除用户数据。"
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

    let _process_id = windows_app::launch(&app.app_user_model_id, &arguments)?;
    cdp::wait_and_inject(port)
}
