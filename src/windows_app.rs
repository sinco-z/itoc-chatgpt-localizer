#[cfg(windows)]
use serde::Deserialize;

#[cfg(windows)]
pub fn show_error(message: &str) {
    show_message(message, true);
}

#[cfg(windows)]
pub fn show_info(message: &str) {
    show_message(message, false);
}

#[cfg(windows)]
fn show_message(message: &str, is_error: bool) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    };

    let title = HSTRING::from("ITOC ChatGPT 中文启动器");
    let message = HSTRING::from(message);
    unsafe {
        let _ = MessageBoxW(
            None,
            &message,
            &title,
            MB_OK | if is_error { MB_ICONERROR } else { MB_ICONINFORMATION },
        );
    }
}

#[cfg(windows)]
pub fn send_voice_typing_shortcut() -> Result<(), String> {
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_H, VK_LWIN,
    };

    let key = |virtual_key, flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [
        key(VK_LWIN, Default::default()),
        key(VK_H, Default::default()),
        key(VK_H, KEYEVENTF_KEYUP),
        key(VK_LWIN, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    (sent == inputs.len() as u32)
        .then_some(())
        .ok_or_else(|| "无法打开 Windows 语音输入。请确认 ChatGPT 窗口在前台且未以管理员身份运行。".to_string())
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub app_user_model_id: String,
    pub version: String,
    pub package_full_name: String,
    pub install_location: String,
}

#[cfg(windows)]
pub fn detect() -> Result<InstalledApp, String> {
    use std::process::Command;

    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction Stop |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($null -eq $pkg) { throw 'OpenAI.Codex package is not installed' }
$apps = @((Get-AppxPackageManifest $pkg).Package.Applications.Application)
$app = $apps | Select-Object -First 1
$appId = [string]$app.Id
if ([string]::IsNullOrWhiteSpace($appId)) { $appId = 'App' }
@{
  appUserModelId = "$($pkg.PackageFamilyName)!$appId"
  version = [string]$pkg.Version
  packageFullName = [string]$pkg.PackageFullName
  installLocation = [string]$pkg.InstallLocation
} | ConvertTo-Json -Compress
"#;

    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .output()
        .map_err(|error| format!("无法调用 PowerShell 检测官方应用：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "没有检测到微软商店安装的官方 ChatGPT 应用".to_string()
        } else {
            format!("检测官方 ChatGPT 应用失败：{detail}")
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析官方应用信息：{error}"))
}

#[cfg(windows)]
pub fn ensure_not_running(app: &InstalledApp) -> Result<(), String> {
    use std::process::Command;

    // App activation reuses an existing ChatGPT process. In that case Windows ignores the
    // remote-debugging arguments, so the launcher cannot inject the temporary locale script.
    // Match the executable path against the package we detected instead of treating every
    // unrelated process named ChatGPT as a conflict.
    const SCRIPT: &str = r#"
$installLocation = ([string]$env:ITOC_CHATGPT_INSTALL_LOCATION).TrimEnd('\')
if ([string]::IsNullOrWhiteSpace($installLocation)) {
  exit 2
}
$processIds = @(
  Get-Process -Name 'ChatGPT' -ErrorAction SilentlyContinue |
    ForEach-Object {
      try {
        $path = [string]$_.Path
        if ($path.StartsWith($installLocation, [System.StringComparison]::OrdinalIgnoreCase)) {
          $_.Id
        }
      }
      catch { }
    }
)
$processIds -join ','
"#;

    let output = Command::new("powershell.exe")
        .env("ITOC_CHATGPT_INSTALL_LOCATION", &app.install_location)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .output()
        .map_err(|error| format!("无法检查 ChatGPT 进程状态：{error}"))?;
    if !output.status.success() {
        return Err("无法检查 ChatGPT 是否已经运行。请先从系统托盘完全退出 ChatGPT 后重试。".to_string());
    }

    let process_ids = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if process_ids.is_empty() {
        return Ok(());
    }

    Err(format!(
        "检测到官方 ChatGPT 已在运行（PID：{process_ids}）。请在系统托盘右键 ChatGPT 并选择“退出”，确认所有 ChatGPT 进程关闭后，再重新运行中文启动器。"
    ))
}

#[cfg(windows)]
pub fn launch(app_user_model_id: &str, arguments: &str) -> Result<u32, String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        ApplicationActivationManager, IApplicationActivationManager, ACTIVATEOPTIONS,
    };

    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninitialize = initialized.is_ok();
        initialized
            .ok()
            .or_else(|error| {
                const RPC_E_CHANGED_MODE: i32 = -2147417850;
                if error.code().0 == RPC_E_CHANGED_MODE {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|error| format!("初始化 Windows 应用启动服务失败：{error}"))?;

        let result: windows::core::Result<u32> = (|| {
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
            manager.ActivateApplication(
                &HSTRING::from(app_user_model_id),
                &HSTRING::from(arguments),
                ACTIVATEOPTIONS(0),
            )
        })();

        if should_uninitialize {
            CoUninitialize();
        }
        result.map_err(|error| format!("启动官方 ChatGPT 应用失败：{error}"))
    }
}
