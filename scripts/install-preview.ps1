[CmdletBinding()]
param(
    [string]$Version = 'v0.1.9-preview.10'
)

$ErrorActionPreference = 'Stop'
$AssetName = 'itoc-chatgpt-zh-windows-x64.exe'
$InstallRoot = Join-Path $env:LOCALAPPDATA 'ITOC\ChatGPTZhPreview'
$ExecutablePath = Join-Path $InstallRoot 'itoc-chatgpt-zh.exe'
$IconPath = Join-Path $InstallRoot 'chatgpt.ico'

function Write-Utf8NoBom([string]$Path, [string]$Content) {
    $utf8 = New-Object -TypeName System.Text.UTF8Encoding -ArgumentList $false
    [IO.File]::WriteAllText($Path, $Content, $utf8)
}

function Assert-Sha256([string]$Path, [string]$Name, [string]$ChecksumsPath) {
    $checksumLine = Get-Content -LiteralPath $ChecksumsPath |
        Where-Object { $_ -match "^[a-fA-F0-9]{64}\s+\*?$([regex]::Escape($Name))$" } |
        Select-Object -First 1
    if (-not $checksumLine) {
        throw "发布包没有包含 $Name 的有效 SHA-256 校验值。"
    }
    $expectedHash = ($checksumLine -split '\s+')[0].ToUpperInvariant()
    $actualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actualHash -cne $expectedHash) {
        throw "$Name 的 SHA-256 校验失败，已拒绝安装。"
    }
}

function Install-StableOfficialIcon([string]$OfficialExecutable, [string]$Destination) {
    $temporaryIcon = "$Destination.tmp"
    Remove-Item -LiteralPath $temporaryIcon -Force -ErrorAction SilentlyContinue
    try {
        $packagedIcon = Join-Path (Split-Path -Parent $OfficialExecutable) 'resources\icon-chatgpt.ico'
        if (Test-Path -LiteralPath $packagedIcon -PathType Leaf) {
            Copy-Item -LiteralPath $packagedIcon -Destination $temporaryIcon -Force
        }
        else {
            Add-Type -AssemblyName System.Drawing
            $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($OfficialExecutable)
            if (-not $icon) {
                throw '无法从官方程序提取图标。'
            }
            try {
                $stream = [IO.File]::Create($temporaryIcon)
                try {
                    $icon.Save($stream)
                }
                finally {
                    $stream.Dispose()
                }
            }
            finally {
                $icon.Dispose()
            }
        }
        Move-Item -LiteralPath $temporaryIcon -Destination $Destination -Force
        return $true
    }
    catch {
        Write-Warning "无法保存稳定的 ChatGPT 图标，将使用启动器默认图标：$($_.Exception.Message)"
        return $false
    }
    finally {
        Remove-Item -LiteralPath $temporaryIcon -Force -ErrorAction SilentlyContinue
    }
}

$officialPackage = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue |
    Sort-Object -Property Version -Descending |
    Select-Object -First 1
if (-not $officialPackage) {
    $downloadUrl = 'https://get.microsoft.com/installer/download/9PLM9XGG6VKS?cid=website_cta_psi'
    Write-Host "官方下载地址：$downloadUrl" -ForegroundColor Cyan
    $openDownload = Read-Host '是否现在打开官方下载页面？ [y/N]'
    if ($openDownload -match '^(?i:y|yes)$') {
        Start-Process $downloadUrl
    }
    throw '未检测到官方 ChatGPT Windows 应用。请先安装后重新运行。'
}
$officialApp = @((Get-AppxPackageManifest -Package $officialPackage.PackageFullName).Package.Applications.Application) |
    Select-Object -First 1
$officialExecutable = Join-Path $officialPackage.InstallLocation $officialApp.Executable
if (-not (Test-Path -LiteralPath $officialExecutable -PathType Leaf)) {
    throw '已检测到 ChatGPT 应用包，但无法定位官方程序文件。请修复或重新安装官方应用。'
}
$releaseUrl = "https://ai-relay.itoc.club/install/releases/$Version"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) "itoc-chatgpt-zh-$([Guid]::NewGuid().ToString('N'))"
$temporaryExe = Join-Path $temporaryRoot $AssetName
$temporaryChecksums = Join-Path $temporaryRoot 'SHA256SUMS.txt'

try {
    New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/$AssetName" -OutFile $temporaryExe
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/SHA256SUMS.txt" -OutFile $temporaryChecksums

    Assert-Sha256 -Path $temporaryExe -Name $AssetName -ChecksumsPath $temporaryChecksums

    $signature = Get-AuthenticodeSignature -LiteralPath $temporaryExe
    if ($signature.Status -eq 'Valid') {
        Write-Host "数字签名有效：$($signature.SignerCertificate.Subject)"
    }
    else {
        if ($signature.SignerCertificate) {
            Write-Host "提示：组件使用自签名证书，Windows 默认不会信任。发布者：$($signature.SignerCertificate.Subject)" -ForegroundColor Yellow
            Write-Host "证书指纹：$($signature.SignerCertificate.Thumbprint)" -ForegroundColor Yellow
        }
        else {
            Write-Host '提示：中文与语音增强组件暂未代码签名，请只从 ai-relay.itoc.club 安装。' -ForegroundColor Yellow
        }
    }

    New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
    Copy-Item -LiteralPath $temporaryExe -Destination $ExecutablePath -Force
    $hasStableIcon = Install-StableOfficialIcon -OfficialExecutable $officialExecutable -Destination $IconPath
    $shortcutIconSource = if ($hasStableIcon) { "$IconPath,0" } else { "$ExecutablePath,0" }

    $shell = New-Object -ComObject WScript.Shell
    $desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ChatGPT 中文版.lnk'
    $startMenuRoot = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ITOC'
    New-Item -ItemType Directory -Path $startMenuRoot -Force | Out-Null
    $legacyDesktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ITOC ChatGPT 中文 Preview.lnk'
    $legacyStartMenuShortcut = Join-Path $startMenuRoot 'ChatGPT 中文 Preview.lnk'
    Remove-Item -LiteralPath $legacyDesktopShortcut -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $legacyStartMenuShortcut -Force -ErrorAction SilentlyContinue
    $shortcutPaths = @($desktopShortcut, (Join-Path $startMenuRoot 'ChatGPT 中文版.lnk'))
    Remove-Item -LiteralPath $shortcutPaths -Force -ErrorAction SilentlyContinue
    foreach ($shortcutPath in $shortcutPaths) {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $ExecutablePath
        $shortcut.WorkingDirectory = $InstallRoot
        $shortcut.Description = '以中文界面和语音输入启动官方 ChatGPT'
        $shortcut.IconLocation = $shortcutIconSource
        $shortcut.Save()
    }

    $uninstallPath = Join-Path $InstallRoot 'uninstall.ps1'
    $uninstallScript = @'
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:LOCALAPPDATA 'ITOC\ChatGPTZhPreview'
$desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ChatGPT 中文版.lnk'
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ITOC\ChatGPT 中文版.lnk'
$legacyDesktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ITOC ChatGPT 中文 Preview.lnk'
$legacyStartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ITOC\ChatGPT 中文 Preview.lnk'
$uninstallShortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ITOC\卸载 ChatGPT 中文版.lnk'
if (Get-Process -Name 'itoc-chatgpt-zh' -ErrorAction SilentlyContinue) {
    Write-Warning '请先从任务栏托盘完全退出 ChatGPT，再运行卸载。'
    Read-Host '按 Enter 关闭窗口'
    exit 1
}
Remove-Item -LiteralPath $desktop, $startMenu, $legacyDesktop, $legacyStartMenu, $uninstallShortcut -Force -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $root -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ne 'uninstall.ps1' } |
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Write-Host '增强启动器、图标和快捷方式已删除。'
Write-Host '官方 ChatGPT、API 配置和历史记录均未修改。'
Read-Host '按 Enter 关闭窗口'
Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $root -Force -ErrorAction SilentlyContinue
'@
    Write-Utf8NoBom -Path $uninstallPath -Content $uninstallScript

    $uninstallShortcut = $shell.CreateShortcut((Join-Path $startMenuRoot '卸载 ChatGPT 中文版.lnk'))
    $uninstallShortcut.TargetPath = (Get-Command 'powershell.exe').Source
    $uninstallShortcut.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$uninstallPath`""
    $uninstallShortcut.WorkingDirectory = [IO.Path]::GetTempPath()
    $uninstallShortcut.Description = '删除 ITOC 中文与语音增强，不影响官方 ChatGPT 和用户数据'
    $uninstallShortcut.Save()

    Write-Host ''
    Write-Host '安装完成。请从任务栏托盘完全退出 ChatGPT，然后双击桌面的：'
    Write-Host 'ChatGPT 中文版'
}
finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}
