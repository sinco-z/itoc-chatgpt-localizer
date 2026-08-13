# ITOC ChatGPT 中文启动器

这是一个面向 Windows 官方 ChatGPT 桌面应用的实验性中文启动器。它通过
Windows 应用包身份启动官方应用，并在仅监听本机回环地址的临时调试端口上启用
官方已经随应用提供的中文资源。

当前状态：**Preview，尚未完成真实 Windows 兼容性验证，也没有代码签名。**

## 安全边界

- 不复制、修改或重新分发 OpenAI 的 EXE、MSIX、ASAR 或语言资源。
- 不读取或修改 `auth.json`、API Key、Provider 配置和历史会话。
- 不要求管理员权限，不添加 Windows 防火墙规则。
- 调试接口显式绑定到 `127.0.0.1`，使用每次启动随机选择的端口。
- 关闭 ChatGPT 后调试端口随官方应用进程一起关闭。
- 原有官方快捷方式保持不变，可随时绕过本启动器。

调试接口仍然具有较高的本机页面控制权限，因此 Preview 版本只适合受邀测试，
不应在不受信任的 Windows 环境中使用。

## 本地开发

```powershell
cargo test
cargo run -- --diagnose
cargo run
```

运行前请从任务栏托盘完全退出 ChatGPT。`--diagnose` 只检测官方安装包，不启动
应用或开放调试端口。

## 安装与卸载

正式 Preview Release 发布后，可以在普通 PowerShell（无需管理员权限）中运行：

```powershell
irm https://ai-relay.itoc.club/install/chatgpt-zh.ps1 | iex
```

由于 Preview 暂未签名，安装器会展示明确警告并要求用户输入确认短语。卸载只删除
ITOC 启动器和快捷方式，不会删除 ChatGPT、`~/.codex` 或聊天历史。

## 兼容性策略

官方应用升级可能改变国际化开关。未知版本发生注入错误时，用户应关闭通过本启动器
打开的 ChatGPT，并改用官方快捷方式。正式发布前会增加按官方版本控制的兼容清单和
失败关闭策略。

