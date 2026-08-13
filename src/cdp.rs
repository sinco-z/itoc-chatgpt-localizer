use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
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
    let (socket, _) =
        connect(websocket_url).map_err(|error| format!("连接本机调试页面失败：{error}"))?;
    let mut client = CdpClient::new(socket)?;

    client.call("Page.enable", json!({}))?;
    client.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": INJECTION_SCRIPT }),
    )?;

    if reload {
        patch_locale_gate_on_reload(&mut client)?;
    } else {
        verify_runtime_marker(&mut client)?;
    }
    let _ = client.socket.close(None);
    Ok(())
}

type CdpSocket = WebSocket<MaybeTlsStream<TcpStream>>;

struct CdpClient {
    socket: CdpSocket,
    next_id: u64,
    events: VecDeque<Value>,
}

impl CdpClient {
    fn new(mut socket: CdpSocket) -> Result<Self, String> {
        if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(750)))
                .map_err(|error| format!("设置调试连接超时失败：{error}"))?;
        }
        Ok(Self {
            socket,
            next_id: 1,
            events: VecDeque::new(),
        })
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
                Ok(event) if event.get("method").is_some() => self.events.push_back(event),
                Ok(_) => {}
                Err(error) if is_timeout(error.as_ref()) && Instant::now() < deadline => {}
                Err(error) => return Err(format!("读取 CDP 命令 {method} 结果失败：{error}")),
            }
            if Instant::now() >= deadline {
                return Err(format!("等待 CDP 命令 {method} 结果超时"));
            }
        }
    }

    fn next_event(&mut self, deadline: Instant) -> Result<Option<Value>, String> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        while Instant::now() < deadline {
            match self.read_value() {
                Ok(event) if event.get("method").is_some() => return Ok(Some(event)),
                Ok(_) => {}
                Err(error) if is_timeout(error.as_ref()) => {}
                Err(error) => return Err(format!("读取 CDP 事件失败：{error}")),
            }
        }
        Ok(None)
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

fn patch_locale_gate_on_reload(client: &mut CdpClient) -> Result<(), String> {
    client.call(
        "Fetch.enable",
        json!({
            "patterns": [{
                "urlPattern": "*",
                "resourceType": "Script",
                "requestStage": "Response"
            }]
        }),
    )?;
    client.call("Page.reload", json!({ "ignoreCache": false }))?;

    let deadline = Instant::now() + Duration::from_secs(25);
    let mut inspected_scripts = 0_u32;
    let mut saw_i18n_source = false;
    let mut patched_url = None;

    while Instant::now() < deadline && patched_url.is_none() {
        let Some(event) = client.next_event(deadline)? else {
            break;
        };
        if event.get("method").and_then(Value::as_str) != Some("Fetch.requestPaused") {
            continue;
        }
        let outcome = handle_paused_script(client, &event)?;
        inspected_scripts += 1;
        saw_i18n_source |= outcome.saw_i18n_source;
        patched_url = outcome.patched_url;
    }

    client.call("Fetch.disable", json!({}))?;
    let Some(url) = patched_url else {
        let detail = if saw_i18n_source {
            "发现 enable_i18n，但当前官方版本的代码形态已变化"
        } else {
            "没有在已加载的前端脚本中发现 enable_i18n"
        };
        return Err(format!(
            "中文门控未修改：{detail}（检查了 {inspected_scripts} 个脚本）。请改用官方快捷方式并反馈应用版本。"
        ));
    };

    verify_runtime_marker(client)?;
    println!("已在内存中启用中文门控：{}", short_url(&url));
    Ok(())
}

#[derive(Default)]
struct PatchOutcome {
    saw_i18n_source: bool,
    patched_url: Option<String>,
}

fn handle_paused_script(client: &mut CdpClient, event: &Value) -> Result<PatchOutcome, String> {
    let params = event
        .get("params")
        .ok_or_else(|| "Fetch.requestPaused 缺少参数".to_string())?;
    let request_id = params
        .get("requestId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Fetch.requestPaused 缺少 requestId".to_string())?;
    let url = params
        .pointer("/request/url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if params.get("responseStatusCode").is_none() {
        client.call("Fetch.continueRequest", json!({ "requestId": request_id }))?;
        return Ok(PatchOutcome::default());
    }

    let response = client.call("Fetch.getResponseBody", json!({ "requestId": request_id }))?;
    let encoded = response
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(|| "Fetch.getResponseBody 缺少 body".to_string())?;
    let body = if response
        .get("base64Encoded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        BASE64
            .decode(encoded)
            .map_err(|error| format!("解析脚本响应失败：{error}"))?
    } else {
        encoded.as_bytes().to_vec()
    };
    let Ok(source) = String::from_utf8(body) else {
        client.call("Fetch.continueRequest", json!({ "requestId": request_id }))?;
        return Ok(PatchOutcome::default());
    };

    let saw_i18n_source = source.contains("enable_i18n");
    let Some(patched) = patch_locale_gate(&source) else {
        client.call("Fetch.continueRequest", json!({ "requestId": request_id }))?;
        return Ok(PatchOutcome {
            saw_i18n_source,
            patched_url: None,
        });
    };

    let status = params
        .get("responseStatusCode")
        .and_then(Value::as_u64)
        .unwrap_or(200);
    let headers = params
        .get("responseHeaders")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|header| {
            let name = header
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "content-encoding" | "content-length" | "transfer-encoding" | "content-md5"
            )
        })
        .collect::<Vec<_>>();
    client.call(
        "Fetch.fulfillRequest",
        json!({
            "requestId": request_id,
            "responseCode": status,
            "responseHeaders": headers,
            "body": BASE64.encode(patched.as_bytes())
        }),
    )?;
    Ok(PatchOutcome {
        saw_i18n_source: true,
        patched_url: Some(url),
    })
}

fn patch_locale_gate(source: &str) -> Option<String> {
    static GATE: OnceLock<Regex> = OnceLock::new();
    let gate = GATE.get_or_init(|| {
        Regex::new(
            r"([A-Za-z_$][A-Za-z0-9_$]*=)[A-Za-z_$][A-Za-z0-9_$]*\?\.get\(`enable_i18n`,!1\)",
        )
        .expect("locale gate regex must compile")
    });
    let patched = gate.replace(source, "${1}!0");
    (patched.as_ref() != source).then(|| patched.into_owned())
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
        .ok_or_else(|| "中文环境脚本没有在重载后的页面生效".to_string())
}

fn short_url(url: &str) -> &str {
    url.rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(url)
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
    fn injection_does_not_touch_credentials_or_provider_state() {
        assert!(INJECTION_SCRIPT.contains("localeOverride"));
        assert!(!INJECTION_SCRIPT.contains("auth.json"));
        assert!(!INJECTION_SCRIPT.contains("API_KEY"));
        assert!(!INJECTION_SCRIPT.contains("model_provider"));
    }

    #[test]
    fn patches_the_current_minified_locale_gate_once() {
        let source = "const a=1;u=n?.get(`enable_i18n`,!1),v=2;";
        let patched = patch_locale_gate(source).expect("current gate should match");
        assert_eq!(patched, "const a=1;u=!0,v=2;");
        assert!(patch_locale_gate(&patched).is_none());
    }

    #[test]
    fn rejects_unknown_locale_gate_shapes() {
        assert!(patch_locale_gate("u=n.get('enable_i18n', false)").is_none());
    }
}
