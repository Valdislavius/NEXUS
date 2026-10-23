$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (Test-Path (Join-Path $root '.tools\cargo\bin')) {
    $env:CARGO_HOME = Join-Path $root '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $root '.tools\rustup'
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
Push-Location $root
try {
    npm.cmd ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    npm.cmd test
    if ($LASTEXITCODE -ne 0) { throw 'UI tests failed' }
    go -C collector test ./...
    if ($LASTEXITCODE -ne 0) { throw 'Go tests failed' }
    go -C collector vet ./...
    if ($LASTEXITCODE -ne 0) { throw 'Go vet failed' }
    npm.cmd run collector:build
    if ($LASTEXITCODE -ne 0) { throw 'Collector build failed' }
    cargo test --manifest-path src-tauri/Cargo.toml --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
    npm.cmd run desktop:build
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
    & (Join-Path $PSScriptRoot 'package.ps1')
    & (Join-Path $PSScriptRoot 'verify-release.ps1')
} finally { Pop-Location }

