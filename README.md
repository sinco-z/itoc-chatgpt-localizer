# ITOC ChatGPT 中文启动器

这是一个面向 Windows 官方 ChatGPT 桌面应用的实验性中文启动器。它通过
Windows 应用包身份启动官方应用，并在仅监听本机回环地址的临时调试端口上，
为页面注册早期运行时脚本，尝试启用官方已经提供的中文资源。

当前状态：**Preview，尚未完成真实 Windows 兼容性验证，也没有代码签名。**

## 安全边界

- 不复制、修改或重新分发 OpenAI 的 EXE、MSIX、ASAR 或语言资源。
- 不修改 WindowsApps 中的官方文件，不拦截或替换网络响应，也不主动刷新页面；
  退出应用后，运行时设置随进程消失。
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

由于 Preview 暂未签名，安装器会展示一条简短提示，但不要求输入确认短语。快捷方式
直接引用本机官方应用内置的多分辨率图标，不复制或重新分发官方图标；卸载只删除
ITOC 启动器和快捷方式，不会删除 ChatGPT、`~/.codex` 或聊天历史。

## 兼容性策略

官方应用升级可能改变国际化开关。启动器只确认早期脚本已经注册并执行，不把它等同于
界面已经完成汉化；实际效果仍需在 Windows 上验证。若官方应用启动异常，用户应关闭
通过本启动器打开的 ChatGPT，并改用官方快捷方式。

全新安装或升级后的首次启动可能较慢。启动器会短暂等待本地设置桥接，并只进行一次有
总时限的设置同步；它在后台观察结果，不会阻塞语音事件或因页面重载提前退出。诊断日志保存在
`%LOCALAPPDATA%\ITOC\ChatGPTZhPreview\launcher.log`，不记录账号、API Key 或聊天内容。
