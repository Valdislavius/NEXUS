$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
New-Item -ItemType Directory -Force (Join-Path $root 'src-tauri\binaries') | Out-Null
Push-Location (Join-Path $root 'collector')
try {
    $env:GOOS = 'windows'
    $env:GOARCH = 'amd64'
    $env:CGO_ENABLED = '0'
    go build -trimpath -ldflags '-s -w' -o (Join-Path $root 'src-tauri\binaries\nexus-collector-x86_64-pc-windows-msvc.exe') ./cmd/nexus-collector
    if ($LASTEXITCODE -ne 0) { throw 'Go build failed' }
} finally { Pop-Location }

# Keep the source corresponding to the embedded LGPL module available to recipients.
Compress-Archive -Path (Join-Path $root 'collector/*') -DestinationPath (Join-Path $root 'docs/PawnIO/NEXUS-collector-source.zip') -Force
