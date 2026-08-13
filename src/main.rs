mod cdp;
mod windows_app;

#[cfg(windows)]
fn main() {
    windows_app::enable_utf8_console();
    if let Err(error) = run() {
        eprintln!("\n错误：{error}");
        eprintln!("请改用官方 ChatGPT 快捷方式启动；本程序不会修改或删除用户数据。");
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

    println!("ITOC ChatGPT 中文启动器 0.1.3 Preview");
    println!("提示：这是未签名测试版本，不会读取 API Key、账号或历史内容。\n");

    let app = windows_app::detect()?;
    println!("已检测到官方应用版本：{}", app.version);
    println!("官方包：{}", app.package_full_name);

    if std::env::args().any(|argument| argument == "--diagnose") {
        println!("检测完成；未启动应用，也未开放调试端口。");
        return Ok(());
    }

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

    println!("正在通过随机本机端口启动 ChatGPT……");
    let process_id = windows_app::launch(&app.app_user_model_id, &arguments)?;
    println!("已请求启动官方应用，PID：{process_id}");
    cdp::wait_and_inject(port)
}
