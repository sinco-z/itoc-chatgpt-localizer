[CmdletBinding()]
param(
    [string]$Version = 'v0.1.8-preview.1'
)

$ErrorActionPreference = 'Stop'
$AssetName = 'itoc-chatgpt-zh-windows-x64.exe'
$IconAssetName = 'itoc-chatgpt-zh.ico'
$InstallRoot = Join-Path $env:LOCALAPPDATA 'ITOC\ChatGPTZhPreview'
$ExecutablePath = Join-Path $InstallRoot 'itoc-chatgpt-zh.exe'
$IconPath = Join-Path $InstallRoot $IconAssetName

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

Write-Host ''
Write-Warning '这是尚未完成代码签名的 Preview 中文与语音增强组件。Windows 可能显示安全警告。'
Write-Host '它会以本机随机调试端口启动官方 ChatGPT；请只在受信任的电脑使用。'

$releaseUrl = "https://ai-relay.itoc.club/install/releases/$Version"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) "itoc-chatgpt-zh-$([Guid]::NewGuid().ToString('N'))"
$temporaryExe = Join-Path $temporaryRoot $AssetName
$temporaryIcon = Join-Path $temporaryRoot $IconAssetName
$temporaryChecksums = Join-Path $temporaryRoot 'SHA256SUMS.txt'

try {
    New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/$AssetName" -OutFile $temporaryExe
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/$IconAssetName" -OutFile $temporaryIcon
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/SHA256SUMS.txt" -OutFile $temporaryChecksums

    Assert-Sha256 -Path $temporaryExe -Name $AssetName -ChecksumsPath $temporaryChecksums
    Assert-Sha256 -Path $temporaryIcon -Name $IconAssetName -ChecksumsPath $temporaryChecksums

    $signature = Get-AuthenticodeSignature -LiteralPath $temporaryExe
    if ($signature.Status -eq 'Valid') {
        Write-Host "数字签名有效：$($signature.SignerCertificate.Subject)"
    }
    else {
        Write-Warning "Preview 当前没有有效 Authenticode 签名：$($signature.Status)"
    }

    New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
    Copy-Item -LiteralPath $temporaryExe -Destination $ExecutablePath -Force
    Copy-Item -LiteralPath $temporaryIcon -Destination $IconPath -Force

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
        $shortcut.IconLocation = "$IconPath,0"
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
Remove-Item -LiteralPath $desktop, $startMenu, $legacyDesktop, $legacyStartMenu -Force -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $root -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ne 'uninstall.ps1' } |
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Write-Host '启动器和快捷方式已删除。可以保留或手动删除：' $root
Write-Host '官方 ChatGPT、API 配置和历史记录均未修改。'
'@
    Write-Utf8NoBom -Path $uninstallPath -Content $uninstallScript

    Write-Host ''
    Write-Host '安装完成。请从任务栏托盘完全退出 ChatGPT，然后双击桌面的：'
    Write-Host 'ChatGPT 中文版'
}
finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}
