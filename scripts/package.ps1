$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $version = (Get-Content package.json -Raw | ConvertFrom-Json).version
    New-Item -ItemType Directory -Force release/installer,release/portable | Out-Null
    $installerFiles = @(Get-ChildItem -LiteralPath release/installer -File)
    if ($installerFiles | Where-Object { $_.Name -ne 'NEXUS-Setup.exe' }) { throw 'release/installer contains unexpected files; move them before packaging' }
    Copy-Item "src-tauri/target/release/bundle/nsis/NEXUS Desktop_${version}_x64-setup.exe" release/installer/NEXUS-Setup.exe -Force
    Copy-Item src-tauri/target/release/nexus-desktop.exe release/portable/NEXUS.exe -Force
    Copy-Item src-tauri/binaries/nexus-collector-x86_64-pc-windows-msvc.exe release/portable/nexus-collector.exe -Force
    Copy-Item README.md release/portable -Force
    Copy-Item docs release/portable -Recurse -Force
    Get-ChildItem -LiteralPath release/portable/docs -Recurse -Force -File | Where-Object { $_.LastWriteTime.Year -lt 1981 } | ForEach-Object { $_.LastWriteTime = [datetime]'2000-01-01' }
    Compress-Archive -Path release/portable/* -DestinationPath "release/NEXUS-Desktop-${version}-portable-windows-x64.zip" -Force
    $hashFiles = @('release/installer/NEXUS-Setup.exe','release/portable/NEXUS.exe','release/portable/nexus-collector.exe',"release/NEXUS-Desktop-${version}-portable-windows-x64.zip")
    $sourcePaths = @('src','tests','collector','scripts','docs','src-tauri/src','src-tauri/icons','src-tauri/capabilities','src-tauri/Cargo.toml','src-tauri/Cargo.lock','src-tauri/build.rs','src-tauri/tauri.conf.json','src-tauri/windows-app-manifest.xml','package.json','package-lock.json','tsconfig.json','vite.config.ts','index.html','README.md','.gitignore')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $sourceZip = Join-Path $root "release/NEXUS-Desktop-${version}-source.zip"
    if (Test-Path -LiteralPath $sourceZip) { Remove-Item -LiteralPath $sourceZip }
    $archive = [IO.Compression.ZipFile]::Open($sourceZip, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($sourcePath in $sourcePaths) {
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
    Get-FileHash $hashFiles -Algorithm SHA256 | ForEach-Object { '{0}  {1}' -f $_.Hash,$_.Path.Substring($root.Length + 1) } | Set-Content "release/SHA256-${version}.txt"
    Get-ChildItem release/installer -File | Select-Object Name,Length,LastWriteTime
} finally { Pop-Location }
