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

# Keep corresponding source available without bundling built executables/caches.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$collectorSourceZip = Join-Path $root 'docs/PawnIO/NEXUS-collector-source.zip'
if (Test-Path -LiteralPath $collectorSourceZip) { Remove-Item -LiteralPath $collectorSourceZip }
$collectorArchive = [IO.Compression.ZipFile]::Open($collectorSourceZip, [IO.Compression.ZipArchiveMode]::Create)
try {
    $collectorSourceRoot = Join-Path $root 'collector'
    foreach ($source in Get-ChildItem -LiteralPath $collectorSourceRoot -Recurse -File) {
        if ($source.Extension -eq '.exe' -or $source.Name -like '*.log' -or $source.Name -like '*.test') { continue }
        $sourceEntry = $source.FullName.Substring($collectorSourceRoot.Length + 1).Replace('\','/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($collectorArchive, $source.FullName, $sourceEntry, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
} finally { $collectorArchive.Dispose() }
