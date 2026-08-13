use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

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
    println!(
        "已连接页面：{} ({})",
        first.title,
        display_target_url(&first.url)
    );
    inject_target(&first)?;
    println!("中文脚本已注册。这个窗口可以最小化；关闭 ChatGPT 后会自动退出。");

    let mut active_url = first.web_socket_debugger_url.unwrap_or_default();
    let mut misses = 0_u8;
    loop {
        thread::sleep(Duration::from_secs(2));
        match find_target(port) {
            Ok(target) => {
                misses = 0;
                let next_url = target.web_socket_debugger_url.clone().unwrap_or_default();
                if !next_url.is_empty() && next_url != active_url {
                    inject_target(&target)?;
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
    let target = targets
        .into_iter()
        .find(is_chatgpt_target)
        .ok_or_else(|| "尚未发现 ChatGPT 页面".to_string())?;
    validate_websocket_url(
        target
            .web_socket_debugger_url
            .as_deref()
            .unwrap_or_default(),
        port,
    )?;
    Ok(target)
}

fn is_chatgpt_target(target: &Target) -> bool {
    if target.target_type != "page"
        || target
            .web_socket_debugger_url
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return false;
    }
    is_primary_app_page(&target.url)
        || is_official_chatgpt_page(&target.title, &target.url)
        || is_chatgpt_bootstrap_page(&target.title, &target.url)
}

fn is_primary_app_page(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    if !normalized.starts_with("app://-/index.html") {
        return false;
    }
    !normalized.contains("initialroute=%2favatar-overlay")
        && !normalized.contains("initialroute=/avatar-overlay")
        && !normalized.contains("initialroute=%2fchatgpt%2fquick-chat")
        && !normalized.contains("initialroute=/chatgpt/quick-chat")
}

fn is_official_chatgpt_page(title: &str, url: &str) -> bool {
    let title = title.trim().to_ascii_lowercase();
    let url = url.trim().to_ascii_lowercase();
    title == "chatgpt"
        && (url == "https://chatgpt.com"
            || url.starts_with("https://chatgpt.com/")
            || url == "https://chat.openai.com"
            || url.starts_with("https://chat.openai.com/"))
}

fn is_chatgpt_bootstrap_page(title: &str, url: &str) -> bool {
    title.trim().eq_ignore_ascii_case("chatgpt")
        && url
            .trim()
            .to_ascii_lowercase()
            .starts_with("data:text/html")
}

fn display_target_url(url: &str) -> &str {
    if url
        .trim()
        .to_ascii_lowercase()
        .starts_with("data:text/html")
    {
        "data:text/html（启动占位页）"
    } else {
        url
    }
}

fn inject_target(target: &Target) -> Result<(), String> {
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| "调试目标没有 WebSocket 地址".to_string())?;
    let (socket, _) =
        connect(websocket_url).map_err(|error| format!("连接本机调试页面失败：{error}"))?;
    let mut client = CdpClient::new(socket)?;

    client.call("Page.enable", json!({}))?;
    client.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": INJECTION_SCRIPT }),
    )?;

    client.call(
        "Runtime.evaluate",
        json!({
            "expression": INJECTION_SCRIPT,
            "returnByValue": true,
            "allowUnsafeEvalBlockedByCSP": true
        }),
    )?;
    verify_runtime_marker(&mut client)?;
    let _ = client.socket.close(None);
    Ok(())
}

type CdpSocket = WebSocket<MaybeTlsStream<TcpStream>>;

struct CdpClient {
    socket: CdpSocket,
    next_id: u64,
}

impl CdpClient {
    fn new(mut socket: CdpSocket) -> Result<Self, String> {
        if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(750)))
                .map_err(|error| format!("设置调试连接超时失败：{error}"))?;
        }
        Ok(Self { socket, next_id: 1 })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "id": id, "method": method, "params": params });
        self.socket
            .send(Message::Text(request.to_string().into()))
            .map_err(|error| format!("发送 CDP 命令 {method} 失败：{error}"))?;

        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match self.read_value() {
                Ok(response) if response.get("id").and_then(Value::as_u64) == Some(id) => {
                    if let Some(error) = response.get("error") {
                        return Err(format!("CDP 命令 {method} 被拒绝：{error}"));
                    }
                    return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                }
                Ok(_) => {}
                Err(error) if is_timeout(error.as_ref()) && Instant::now() < deadline => {}
                Err(error) => return Err(format!("读取 CDP 命令 {method} 结果失败：{error}")),
            }
            if Instant::now() >= deadline {
                return Err(format!("等待 CDP 命令 {method} 结果超时"));
            }
        }
    }

    fn read_value(&mut self) -> Result<Value, Box<tungstenite::Error>> {
        loop {
            let message = self.socket.read().map_err(Box::new)?;
            if let Message::Text(text) = message {
                return serde_json::from_str(text.as_ref()).map_err(|error| {
                    Box::new(tungstenite::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error,
                    )))
                });
            }
        }
    }
}

fn is_timeout(error: &tungstenite::Error) -> bool {
    matches!(
        error,
        tungstenite::Error::Io(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
    )
}

fn verify_runtime_marker(client: &mut CdpClient) -> Result<(), String> {
    let result = client.call(
        "Runtime.evaluate",
        json!({
            "expression": "Boolean(globalThis.__ITOC_ZH_PREVIEW__?.locale === 'zh-CN')",
            "returnByValue": true
        }),
    )?;
    if result.get("exceptionDetails").is_some() {
        return Err("中文环境脚本执行异常".to_string());
    }
    let active = result
        .pointer("/result/value")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    active
        .then_some(())
        .ok_or_else(|| "中文环境脚本没有在页面生效".to_string())
}

fn validate_websocket_url(url: &str, expected_port: u16) -> Result<(), String> {
    let uri = url
        .parse::<tungstenite::http::Uri>()
        .map_err(|error| format!("调试页面返回了无效 WebSocket 地址：{error}"))?;
    if uri.scheme_str() != Some("ws") {
        return Err("调试 WebSocket 必须使用本机 ws 协议".to_string());
    }
    let host = uri
        .host()
        .unwrap_or_default()
        .trim_start_matches('[')
        .trim_end_matches(']');
    if !matches!(host, "127.0.0.1" | "::1") {
        return Err("调试 WebSocket 地址不是本机回环地址，已拒绝连接".to_string());
    }
    if uri.port_u16() != Some(expected_port) {
        return Err("调试 WebSocket 端口与启动器选择的端口不一致".to_string());
    }
    Ok(())
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

    const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if response.len().saturating_add(read) > MAX_RESPONSE_BYTES {
                    return Err("调试服务响应超过安全上限".to_string());
                }
                response.extend_from_slice(&chunk[..read]);
                if http_response_complete(&response)? {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !response.is_empty() =>
            {
                break;
            }
            Err(error) => return Err(format!("读取调试目标失败：{error}")),
        }
    }
    parse_http_response(&response)
}

fn http_response_complete(response: &[u8]) -> Result<bool, String> {
    let Some(header_end) = find_header_end(response) else {
        return Ok(false);
    };
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|error| format!("调试服务响应头不是 UTF-8：{error}"))?;
    let Some(content_length) = content_length(headers)? else {
        return Ok(false);
    };
    Ok(response.len() >= header_end + 4 + content_length)
}

fn parse_http_response(response: &[u8]) -> Result<String, String> {
    let header_end =
        find_header_end(response).ok_or_else(|| "调试服务返回了无效 HTTP 响应".to_string())?;
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|error| format!("调试服务响应头不是 UTF-8：{error}"))?;
    if !headers.lines().next().unwrap_or_default().contains(" 200 ") {
        return Err(format!(
            "调试服务返回异常状态：{}",
            headers.lines().next().unwrap_or_default()
        ));
    }
    let body = &response[header_end + 4..];
    let body = if let Some(length) = content_length(headers)? {
        body.get(..length)
            .ok_or_else(|| "调试服务响应正文不完整".to_string())?
    } else {
        body
    };
    String::from_utf8(body.to_vec()).map_err(|error| format!("调试服务正文不是 UTF-8：{error}"))
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> Result<Option<usize>, String> {
    headers
        .lines()
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim())
        })
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|error| format!("调试服务 Content-Length 无效：{error}"))
        })
        .transpose()
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
    fn selects_stable_app_and_chatgpt_bootstrap_pages() {
        let stable = Target {
            target_type: "page".to_string(),
            title: "Codex".to_string(),
            url: "app://-/index.html".to_string(),
            web_socket_debugger_url: Some("ws://127.0.0.1:1234/devtools/page/a".to_string()),
        };
        assert!(is_chatgpt_target(&stable));

        let placeholder = Target {
            title: "ChatGPT".to_string(),
            url: "data:text/html;charset=utf-8,loading".to_string(),
            ..stable.clone()
        };
        assert!(is_chatgpt_target(&placeholder));

        let unrelated_placeholder = Target {
            title: "Example".to_string(),
            ..placeholder
        };
        assert!(!is_chatgpt_target(&unrelated_placeholder));

        let quick_chat = Target {
            url: "app://-/index.html?initialRoute=%2Fchatgpt%2Fquick-chat-prewarm".to_string(),
            ..stable
        };
        assert!(!is_chatgpt_target(&quick_chat));
    }

    #[test]
    fn injection_does_not_touch_credentials_or_provider_state() {
        assert!(INJECTION_SCRIPT.contains("localeOverride"));
        assert!(!INJECTION_SCRIPT.contains("auth.json"));
        assert!(!INJECTION_SCRIPT.contains("API_KEY"));
        assert!(!INJECTION_SCRIPT.contains("model_provider"));
    }

    #[test]
    fn injection_uses_early_runtime_hooks_without_network_interception() {
        assert!(INJECTION_SCRIPT.contains("__STATSIG__"));
        assert!(INJECTION_SCRIPT.contains("getDynamicConfig"));
        assert!(INJECTION_SCRIPT.contains("enable_i18n"));
        assert!(!INJECTION_SCRIPT.contains("Fetch.enable"));
        assert!(!INJECTION_SCRIPT.contains("Fetch.fulfillRequest"));
    }

    #[test]
    fn parses_complete_http_body_without_waiting_for_disconnect() {
        let body = r#"[{"type":"page"}]"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        assert!(http_response_complete(response.as_bytes()).unwrap());
        assert_eq!(parse_http_response(response.as_bytes()).unwrap(), body);
    }

    #[test]
    fn rejects_truncated_http_body() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{}";
        assert!(!http_response_complete(response).unwrap());
        assert!(parse_http_response(response).is_err());
    }

    #[test]
    fn accepts_only_the_expected_loopback_websocket() {
        assert!(validate_websocket_url("ws://127.0.0.1:43123/devtools/page/one", 43123).is_ok());
        assert!(validate_websocket_url("ws://example.com:43123/devtools/page/one", 43123).is_err());
        assert!(validate_websocket_url("ws://127.0.0.1:43124/devtools/page/one", 43123).is_err());
    }
}
