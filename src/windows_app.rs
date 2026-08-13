#[cfg(windows)]
use serde::Deserialize;

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub app_user_model_id: String,
    pub version: String,
    pub package_full_name: String,
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
