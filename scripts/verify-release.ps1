$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NexusPEResource {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
  [DllImport("kernel32.dll")] public static extern uint SizeofResource(IntPtr module, IntPtr resource);
  [DllImport("kernel32.dll")] public static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
  [DllImport("kernel32.dll")] public static extern IntPtr LockResource(IntPtr resource);
  [DllImport("kernel32.dll")] public static extern bool FreeLibrary(IntPtr module);
}
'@
function Get-EmbeddedManifest([string] $path) {
    $module = [NexusPEResource]::LoadLibraryEx($path, [IntPtr]::Zero, 0x22)
    if ($module -eq [IntPtr]::Zero) { throw "Cannot inspect PE resource: $path" }
    try {
        $resource = [NexusPEResource]::FindResource($module, [IntPtr]1, [IntPtr]24)
        if ($resource -eq [IntPtr]::Zero) { throw "No embedded manifest: $path" }
        $length = [NexusPEResource]::SizeofResource($module, $resource)
        $data = [NexusPEResource]::LockResource([NexusPEResource]::LoadResource($module, $resource))
        $bytes = New-Object byte[] $length
        [Runtime.InteropServices.Marshal]::Copy($data, $bytes, 0, $length)
        return [Text.Encoding]::UTF8.GetString($bytes).Trim([char]0)
    } finally { [NexusPEResource]::FreeLibrary($module) | Out-Null }
}
$taskConfig = Get-Content (Join-Path $taskRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$taskVersion = $taskConfig.version
if ($taskConfig.bundle.windows.nsis.installMode -ne 'currentUser') { throw 'Installer is not currentUser' }
if ($taskConfig.bundle.windows.webviewInstallMode.type -ne 'offlineInstaller') { throw 'Offline WebView2 missing' }
$appPath = Join-Path $taskRoot 'release/portable/NEXUS.exe'
$installerPath = Join-Path $taskRoot 'release/installer/NEXUS-Setup.exe'
$appManifest = Get-EmbeddedManifest $appPath
$installerManifest = Get-EmbeddedManifest $installerPath
if ($appManifest -notmatch 'requestedExecutionLevel[^>]+level="requireAdministrator"' -or $appManifest -notmatch 'uiAccess="false"') { throw 'Release application UAC manifest is incorrect' }
if ($appManifest -notmatch 'Microsoft.Windows.Common-Controls') { throw 'Common Controls manifest dependency missing' }
if ($installerManifest -notmatch 'requestedExecutionLevel[^>]+level="asInvoker"') { throw 'Installer requests elevation unexpectedly' }
if ((Get-Item $appPath).VersionInfo.ProductVersion -ne $taskVersion) { throw 'Application version mismatch' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $taskRoot "release/NEXUS-Desktop-${taskVersion}-portable-windows-x64.zip"))
try {
    if (-not ($zip.Entries | Where-Object FullName -eq 'NEXUS.exe')) { throw 'Portable application missing' }
    if (-not ($zip.Entries | Where-Object FullName -eq 'nexus-collector.exe')) { throw 'Portable collector missing' }
} finally { $zip.Dispose() }
$taskNsis = Get-ChildItem (Join-Path $taskRoot 'src-tauri/target/release') -Recurse -Filter '*.nsi' | Select-Object -First 1
if (-not $taskNsis) { throw 'Generated NSIS script unavailable' }
$nsis = Get-Content -LiteralPath $taskNsis.FullName -Raw
if ($nsis -notmatch '!define INSTALLMODE "currentUser"' -or $nsis -notmatch '!define INSTALLWEBVIEW2MODE "offlineInstaller"') { throw 'NSIS installation mode mismatch' }
if ($nsis -notmatch 'nexus-collector.exe') { throw 'Bundled collector missing from installer' }
$webviewMatch = [regex]::Match($nsis, '!define WEBVIEW2INSTALLERPATH "([^"]+)"')
if (-not $webviewMatch.Success -or -not (Test-Path -LiteralPath $webviewMatch.Groups[1].Value)) { throw 'Embedded offline WebView2 payload not found' }
$signature = Get-AuthenticodeSignature -LiteralPath $webviewMatch.Groups[1].Value
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { throw 'WebView2 installer signature invalid' }
$taskArtifacts = Join-Path $taskRoot 'artifacts/alpha031'
New-Item -ItemType Directory -Path $taskArtifacts -Force | Out-Null
$appManifest | Set-Content (Join-Path $taskArtifacts 'application.manifest.xml')
$installerManifest | Set-Content (Join-Path $taskArtifacts 'installer.manifest.xml')
$result = [ordered]@{ version=$taskVersion; applicationExecutionLevel='requireAdministrator'; installerExecutionLevel='asInvoker'; installMode='currentUser'; offlineWebView2Embedded=$true; webView2Signature='Valid Microsoft Corporation'; collectorBundled=$true; portablePairPresent=$true }
$result | ConvertTo-Json | Set-Content (Join-Path $taskArtifacts 'release-verification.json')
$result | ConvertTo-Json
