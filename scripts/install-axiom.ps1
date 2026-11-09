param(
    [switch]$SkipBuild,
    [string]$CertThumbprint
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipBuild) {
        cargo build -p axiom-cli --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    }
    & (Join-Path $PSScriptRoot 'sign-axiom.ps1') `
        -Path 'target\release\axiom.exe' `
        -CertThumbprint $CertThumbprint `
        -Install
    Write-Output "Run 'axiom version' in a new terminal to verify the installation."
} finally {
    Pop-Location
}
