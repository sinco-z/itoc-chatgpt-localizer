use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{connect, Message};

const INJECTION_SCRIPT: &str = include_str!("injection.js");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    #[serde(rename = "type")]
    target_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    web_socket_debugger_url: Option<String>,
}

pub fn wait_and_inject(port: u16) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let first = loop {
        match find_target(port) {
            Ok(target) => break target,
            Err(error) if Instant::now() < deadline => {
                eprint!("\r等待 ChatGPT 页面启动：{error}        ");
                thread::sleep(Duration::from_millis(500));
            }
            Err(error) => return Err(format!("等待 ChatGPT 调试页面超时：{error}")),
        }
    };
    eprintln!();
    inject_target(&first, true)?;
    println!("中文 Preview 已注入。这个窗口可以最小化；关闭 ChatGPT 后会自动退出。");

    let mut active_url = first.web_socket_debugger_url.unwrap_or_default();
    let mut misses = 0_u8;
    loop {
        thread::sleep(Duration::from_secs(2));
        match find_target(port) {
            Ok(target) => {
                misses = 0;
                let next_url = target.web_socket_debugger_url.clone().unwrap_or_default();
                if !next_url.is_empty() && next_url != active_url {
                    inject_target(&target, true)?;
                    active_url = next_url;
                    println!("检测到 ChatGPT 页面重建，已重新注入中文 Preview。");
                }
            }
            Err(_) => {
                misses += 1;
                if misses >= 8 {
                    println!("ChatGPT 已关闭，中文启动器退出。");
                    return Ok(());
                }
            }
        }
    }
}

fn find_target(port: u16) -> Result<Target, String> {
    let body = http_get_local(port, "/json")?;
    let targets: Vec<Target> =
        serde_json::from_str(&body).map_err(|error| format!("无法解析调试目标：{error}"))?;
    targets
        .into_iter()
        .find(is_chatgpt_target)
        .ok_or_else(|| "尚未发现 ChatGPT 页面".to_string())
}

fn is_chatgpt_target(target: &Target) -> bool {
    if target.target_type != "page" || target.web_socket_debugger_url.is_none() {
        return false;
    }
    let identity = format!("{} {}", target.title, target.url).to_ascii_lowercase();
    identity.contains("codex")
        || identity.contains("chatgpt")
        || target.url.starts_with("https://chatgpt.com/")
        || target.url.starts_with("https://chat.openai.com/")
        || target.url.starts_with("data:text/html")
}

fn inject_target(target: &Target, reload: bool) -> Result<(), String> {
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| "调试目标没有 WebSocket 地址".to_string())?;
    let (mut socket, _) =
        connect(websocket_url).map_err(|error| format!("连接本机调试页面失败：{error}"))?;

    cdp_call(&mut socket, 1, "Page.enable", json!({}))?;
    cdp_call(
        &mut socket,
        2,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": INJECTION_SCRIPT }),
    )?;
    cdp_call(
        &mut socket,
        3,
        "Runtime.evaluate",
        json!({
            "expression": INJECTION_SCRIPT,
            "returnByValue": true,
            "awaitPromise": true
        }),
    )?;

    if reload {
        let request = json!({
            "id": 4,
            "method": "Page.reload",
            "params": { "ignoreCache": false }
        });
        socket
            .send(Message::Text(request.to_string().into()))
            .map_err(|error| format!("请求 ChatGPT 刷新失败：{error}"))?;
    }
    let _ = socket.close(None);
    Ok(())
}

fn cdp_call(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let request = json!({ "id": id, "method": method, "params": params });
    socket
        .send(Message::Text(request.to_string().into()))
        .map_err(|error| format!("发送 CDP 命令 {method} 失败：{error}"))?;

    loop {
        let message = socket
            .read()
            .map_err(|error| format!("读取 CDP 命令 {method} 结果失败：{error}"))?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: Value = serde_json::from_str(text.as_ref())
            .map_err(|error| format!("CDP 返回了无效 JSON：{error}"))?;
        if response.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(format!("CDP 命令 {method} 被拒绝：{error}"));
        }
        return Ok(response.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn http_get_local(port: u16, path: &str) -> Result<String, String> {
    let address = ("127.0.0.1", port)
        .to_socket_addrs()
        .map_err(|error| format!("解析本机调试地址失败：{error}"))?
        .next()
        .ok_or_else(|| "本机调试地址为空".to_string())?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))
        .map_err(|error| format!("调试端口尚未就绪：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("设置调试读取超时失败：{error}"))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| format!("请求调试目标失败：{error}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("读取调试目标失败：{error}"))?;
    let response = String::from_utf8(response)
        .map_err(|error| format!("调试服务返回了非 UTF-8 数据：{error}"))?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "调试服务返回了无效 HTTP 响应".to_string())?;
    if !headers.lines().next().unwrap_or_default().contains(" 200 ") {
        return Err(format!(
            "调试服务返回异常状态：{}",
            headers.lines().next().unwrap_or_default()
        ));
    }
    Ok(body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_only_expected_chatgpt_pages() {
        let target = Target {
            target_type: "page".to_string(),
            title: "ChatGPT".to_string(),
            url: "https://chatgpt.com/codex".to_string(),
            web_socket_debugger_url: Some("ws://127.0.0.1:1234/devtools/page/a".to_string()),
        };
        assert!(is_chatgpt_target(&target));

        let unrelated = Target {
            title: "Example".to_string(),
            url: "https://example.com".to_string(),
            ..target
        };
        assert!(!is_chatgpt_target(&unrelated));
    }

    #[test]
    fn injection_keeps_the_feature_flag_isolated() {
        assert!(INJECTION_SCRIPT.contains("72216192"));
        assert!(INJECTION_SCRIPT.contains("enable_i18n"));
        assert!(!INJECTION_SCRIPT.contains("auth.json"));
        assert!(!INJECTION_SCRIPT.contains("API_KEY"));
    }
}
