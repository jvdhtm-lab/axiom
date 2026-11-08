param(
    [string]$CertThumbprint = '14EDB8E45DC1F916AD782B2B2BB31875F5378992',
    [switch]$SkipToolchain
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $SkipToolchain) {
    & (Join-Path $PSScriptRoot 'fix-windows-toolchain.ps1') -CertThumbprint $CertThumbprint
}

$cargoDir = Join-Path $root '.cargo'
if (-not (Test-Path -LiteralPath $cargoDir)) { New-Item -ItemType Directory -Path $cargoDir | Out-Null }
$config = Join-Path $cargoDir 'config.toml'
if (Test-Path -LiteralPath $config) { Copy-Item -LiteralPath $config -Destination "$config.bak" -Force }

$runner = (Resolve-Path (Join-Path $PSScriptRoot 'cargo-runner.ps1')).Path
$escaped = $runner -replace '\\', '\\'
@"
[target.x86_64-pc-windows-gnu]
runner = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "$escaped"]
"@ | Set-Content -LiteralPath $config -Encoding ascii

$exclude = Join-Path $root '.git\info\exclude'
if (Test-Path -LiteralPath $exclude) {
    foreach ($pattern in @('.cargo/', '.cargo/config.toml')) {
        if (-not (Select-String -LiteralPath $exclude -Pattern "^$([regex]::Escape($pattern))$" -Quiet)) {
            Add-Content -LiteralPath $exclude -Value $pattern
        }
    }
}

Write-Output "Configured Cargo runner: $runner"
