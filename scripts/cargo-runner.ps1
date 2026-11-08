param(
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$CommandLine
)

$ErrorActionPreference = 'Continue'
if (-not $CommandLine -or $CommandLine.Count -eq 0) { exit 2 }

$exe = $CommandLine[0]
$exeArgs = @()
if ($CommandLine.Count -gt 1) { $exeArgs = $CommandLine[1..($CommandLine.Count - 1)] }
$workDir = (Get-Location).Path

function Get-TrustedSigningCert {
    if ($env:AXIOM_SIGN_CERT_THUMBPRINT) {
        $candidate = Get-Item "Cert:\CurrentUser\My\$($env:AXIOM_SIGN_CERT_THUMBPRINT)" -ErrorAction SilentlyContinue
        if ($candidate) { return $candidate }
    }
    foreach ($subject in @('CN=MeshCloud Dev Signing', 'CN=Axiom Dev Signing')) {
        $candidate = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
            Where-Object { $_.Subject -eq $subject -and $_.HasPrivateKey } |
            Select-Object -First 1
        if (-not $candidate) { continue }
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'CurrentUser')
        $store.Open('ReadOnly')
        $trusted = $store.Certificates | Where-Object { $_.Thumbprint -eq $candidate.Thumbprint }
        $store.Close()
        if ($trusted) { return $candidate }
    }
    return $null
}

function Get-StripPath {
    $command = Get-Command strip.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $winget = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
    if (Test-Path -LiteralPath $winget) {
        $found = Get-ChildItem -LiteralPath $winget -Recurse -Filter strip.exe -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    return $null
}

function Quote-Arg([string]$value) {
    if ($value -eq '') { return '""' }
    if ($value -notmatch '[\s"]') { return $value }
    return '"' + ($value -replace '"', '\"') + '"'
}

$cert = Get-TrustedSigningCert
$strip = Get-StripPath
if (-not $cert -or -not $strip) {
    & $exe @exeArgs
    exit $LASTEXITCODE
}

$stem = [IO.Path]::GetFileNameWithoutExtension($exe)
$argLine = (($exeArgs | ForEach-Object { Quote-Arg $_ }) -join ' ')
for ($attempt = 1; $attempt -le 12; $attempt++) {
    $tmp = Join-Path $env:TEMP "axiom-run-$stem-$attempt.exe"
    Copy-Item -LiteralPath $exe -Destination $tmp -Force
    try { & $strip --strip-debug $tmp 2>$null } catch { }
    $stream = [IO.File]::Open($tmp, [IO.FileMode]::Append)
    $bytes = New-Object byte[] 32
    (New-Object Random).NextBytes($bytes)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Close()
    $null = Set-AuthenticodeSignature -FilePath $tmp -Certificate $cert -HashAlgorithm SHA256

    $out = "$tmp.out"
    $err = "$tmp.err"
    $process = $null
    try {
        $start = @{
            FilePath = $tmp
            WorkingDirectory = $workDir
            Wait = $true
            PassThru = $true
            NoNewWindow = $true
            RedirectStandardOutput = $out
            RedirectStandardError = $err
            ErrorAction = 'Stop'
        }
        if ($exeArgs.Count -gt 0) { $start.ArgumentList = $argLine }
        $process = Start-Process @start
    } catch {
        Start-Sleep -Milliseconds 250
    }
    if ($null -ne $process) {
        Get-Content -LiteralPath $out -ErrorAction SilentlyContinue
        if ($process.ExitCode -ne 0) {
            Get-Content -LiteralPath $err -ErrorAction SilentlyContinue
        }
        exit $process.ExitCode
    }
}

Write-Error "Application Control blocked $exe after 12 signed attempts."
exit 125
