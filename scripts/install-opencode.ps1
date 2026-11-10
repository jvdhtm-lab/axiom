param(
    [string]$Url = 'https://opencode.ai/update/api/latest/cli/opencode/opencode-windows-x64.zip'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tools = Join-Path $root '.tools\opencode'
$exe = Join-Path $tools 'opencode.exe'
$zip = Join-Path $tools 'opencode.zip'

if (-not (Test-Path -LiteralPath $tools)) {
    New-Item -ItemType Directory -Path $tools | Out-Null
}

if (-not (Test-Path -LiteralPath $exe)) {
    Write-Output "Downloading the OpenCode CLI…"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $zip
    Expand-Archive -LiteralPath $zip -DestinationPath $tools -Force
}

if ($env:OS -eq 'Windows_NT') {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'sign-axiom.ps1') -Path $exe | Out-Null
}

& $exe --version
if ($LASTEXITCODE -ne 0) { throw "OpenCode CLI did not run" }
Write-Output "Installed OpenCode CLI: $exe"
Write-Output "Set OPENCODE_BIN=$exe for the UI demo."
