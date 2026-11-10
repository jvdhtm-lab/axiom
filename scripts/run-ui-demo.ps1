param(
    [int]$Port = 8099,
    [switch]$Release,
    [switch]$NoOpen,
    [switch]$Record,
    [switch]$Detach,
    [switch]$Stop,
    [ValidateSet('template', 'opencode')]
    [string]$Agent = 'template'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$pidFile = Join-Path $env:TEMP 'axiom-ui-demo.pid'
$url = "http://127.0.0.1:$Port/"

function Stop-DemoProcess {
    $processId = if (Test-Path -LiteralPath $pidFile) {
        Get-Content -LiteralPath $pidFile -ErrorAction SilentlyContinue
    } else {
        $null
    }
    if ($processId) {
        $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($process) {
            Write-Output "Stopping Axiom UI demo (PID $processId)…"
            if ($IsWindows -or $env:OS -eq 'Windows_NT') {
                & taskkill /PID $processId /T /F 2>$null | Out-Null
            } else {
                $process | Stop-Process -Force
            }
        }
        Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
    }
}

if ($Stop) {
    Stop-DemoProcess
    exit 0
}

$listener = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if ($listener) {
    Write-Error "Port $Port is already in use. Use -Port <port> or run with -Stop to stop the previous demo."
}

Push-Location $root
try {
    $profileArgs = @()
    if ($Release) { $profileArgs += '--release' }
    & cargo build @profileArgs -p axiom-server --example ui_agent_demo
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $profile = if ($Release) { 'release' } else { 'debug' }
    $exe = Join-Path $root "target\$profile\examples\ui_agent_demo.exe"
    if (-not (Test-Path -LiteralPath $exe)) {
        $exe = Join-Path $root "target\$profile\examples\ui_agent_demo"
    }
    if (-not (Test-Path -LiteralPath $exe)) {
        throw "Cannot find the demo binary under target\$profile\examples"
    }

    if ($env:OS -eq 'Windows_NT') {
        try {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'sign-axiom.ps1') -Path $exe | Out-Null
        } catch {
            Write-Warning "Could not sign the demo binary: $($_.Exception.Message)"
        }
    }

    $env:AXIOM_UI_AGENT = $Agent
    if ($Agent -eq 'opencode') {
        $opencode = Join-Path $root '.tools\opencode\opencode.exe'
        if (-not (Test-Path -LiteralPath $opencode)) {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'install-opencode.ps1')
        }
        $env:OPENCODE_BIN = $opencode
    }

    $stdout = Join-Path $env:TEMP "axiom-ui-demo-$Port.out.log"
    $stderr = Join-Path $env:TEMP "axiom-ui-demo-$Port.err.log"
    $server = Start-Process -FilePath $exe -ArgumentList @('--bind', "127.0.0.1:$Port") -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    Set-Content -LiteralPath $pidFile -Value $server.Id -Encoding ascii

    $ready = $false
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        Start-Sleep -Milliseconds 200
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri $url -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                $ready = $true
                break
            }
        } catch {
        }
        if ($server.HasExited) { break }
    }
    if (-not $ready) {
        Write-Error "The demo did not start. See $stderr and $stdout."
    }

    Write-Output "Axiom UI agent demo: $url"
    Write-Output "Agent mode: $Agent"
    Write-Output "Server PID: $($server.Id)"
    if ($Detach) {
        Write-Output "Running in the background. Stop it with:"
        Write-Output "  powershell -File scripts/run-ui-demo.ps1 -Stop"
        exit 0
    }

    if (-not $NoOpen) {
        if ($Record) {
            $candidates = @(
                $env:AXIOM_CHROME,
                "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
                "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
                "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
                "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
            ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
            $browser = $candidates | Select-Object -First 1
            if ($browser) {
                $userData = Join-Path $env:TEMP "axiom-ui-demo-chrome-$Port"
                Start-Process -FilePath $browser -ArgumentList @(
                    "--app=$url",
                    '--window-size=1280,900',
                    "--user-data-dir=$userData"
                ) | Out-Null
            } else {
                Write-Warning "No Chrome/Edge found; opening the default browser instead."
                Start-Process $url
            }
        } else {
            Start-Process $url
        }
    }

    Write-Output "Press Ctrl+C to stop the demo."
    try {
        while (-not $server.HasExited) {
            Start-Sleep -Seconds 1
        }
    } finally {
        if (-not $Detach) {
            Stop-DemoProcess
        }
    }
} finally {
    Pop-Location
}
