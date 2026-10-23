$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $version = (Get-Content package.json -Raw | ConvertFrom-Json).version
    $releaseRoot = Join-Path $root "release/$version"
    $installerRoot = Join-Path $releaseRoot 'installer'
    $portableRoot = Join-Path $releaseRoot 'portable'
    New-Item -ItemType Directory -Force $installerRoot,$portableRoot | Out-Null
    $installerFiles = @(Get-ChildItem -LiteralPath $installerRoot -File)
    if ($installerFiles | Where-Object { $_.Name -ne 'NEXUS-Setup.exe' }) { throw 'release/installer contains unexpected files; move them before packaging' }
    Copy-Item "src-tauri/target/release/bundle/nsis/NEXUS Desktop_${version}_x64-setup.exe" (Join-Path $installerRoot 'NEXUS-Setup.exe') -Force
    Copy-Item src-tauri/target/release/nexus-desktop.exe (Join-Path $portableRoot 'NEXUS.exe') -Force
    Copy-Item src-tauri/binaries/nexus-collector-x86_64-pc-windows-msvc.exe (Join-Path $portableRoot 'nexus-collector.exe') -Force
    Copy-Item README.md $portableRoot -Force
    Copy-Item docs $portableRoot -Recurse -Force
    Get-ChildItem -LiteralPath (Join-Path $portableRoot 'docs') -Recurse -Force -File | Where-Object { $_.LastWriteTime.Year -lt 1981 } | ForEach-Object { $_.LastWriteTime = [datetime]'2000-01-01' }
    $portableZip = Join-Path $releaseRoot "NEXUS-Desktop-${version}-portable-windows-x64.zip"
    Compress-Archive -Path "$portableRoot/*" -DestinationPath $portableZip -Force
    $hashFiles = @((Join-Path $installerRoot 'NEXUS-Setup.exe'),(Join-Path $portableRoot 'NEXUS.exe'),(Join-Path $portableRoot 'nexus-collector.exe'),$portableZip)
    # Package the same ordinary source tree as Git, including the independent
    # website. Dependencies, databases, tokens and generated files are ignored.
    $sourcePaths = @(git ls-files --cached --others --exclude-standard | Sort-Object -Unique)
    if ($LASTEXITCODE -ne 0 -or $sourcePaths.Count -lt 10) { throw 'Cannot enumerate repository sources' }
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $sourceZip = Join-Path $releaseRoot "NEXUS-Desktop-${version}-source.zip"
    if (Test-Path -LiteralPath $sourceZip) { Remove-Item -LiteralPath $sourceZip }
    $archive = [IO.Compression.ZipFile]::Open($sourceZip, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($sourcePath in $sourcePaths) {
            if (-not (Test-Path -LiteralPath (Join-Path $root $sourcePath))) { continue }
            $sourceItem = Get-Item -LiteralPath (Join-Path $root $sourcePath)
            $sourceFiles = if ($sourceItem.PSIsContainer) { Get-ChildItem -LiteralPath $sourceItem.FullName -Recurse -File } else { @($sourceItem) }
            foreach ($sourceFile in $sourceFiles) {
                if ($sourceFile.Extension -eq '.exe' -or $sourceFile.Name -like '*.tsbuildinfo') { continue }
                $entry = $sourceFile.FullName.Substring($root.Length + 1).Replace('\','/')
                [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $sourceFile.FullName, $entry) | Out-Null
            }
        }
    } finally { $archive.Dispose() }
    $hashFiles += $sourceZip
    Get-FileHash $hashFiles -Algorithm SHA256 | ForEach-Object { '{0}  {1}' -f $_.Hash,$_.Path.Substring($releaseRoot.Length + 1) } | Set-Content (Join-Path $releaseRoot "SHA256-${version}.txt")
    Get-ChildItem $installerRoot -File | Select-Object Name,Length,LastWriteTime
} finally { Pop-Location }
