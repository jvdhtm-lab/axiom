param(
    [string]$Gcc = 'x86_64-w64-mingw32-gcc',
    [string]$CertThumbprint = '14EDB8E45DC1F916AD782B2B2BB31875F5378992'
)

$ErrorActionPreference = 'Stop'
$gccCommand = Get-Command $Gcc -ErrorAction SilentlyContinue
if (-not $gccCommand) { throw "Cannot find $Gcc on PATH." }
$mingw = Split-Path -Parent (Split-Path -Parent $gccCommand.Source)
$ld = Join-Path $mingw 'x86_64-w64-mingw32\bin\ld.exe'
if (-not (Test-Path -LiteralPath $ld)) { throw "Cannot find $ld" }

$cert = Get-Item "Cert:\CurrentUser\My\$CertThumbprint" -ErrorAction SilentlyContinue
if (-not $cert) { throw "Code-signing certificate $CertThumbprint is not in Cert:\CurrentUser\My." }
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'CurrentUser')
$store.Open('ReadOnly')
$trusted = $store.Certificates | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
$store.Close()
if (-not $trusted) { throw "Certificate $CertThumbprint is not trusted in CurrentUser\TrustedPublisher." }

$signature = Get-AuthenticodeSignature -LiteralPath $ld
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $cert.Thumbprint) {
    $backup = "$ld.unsigned.bak"
    if (-not (Test-Path -LiteralPath $backup)) { Copy-Item -LiteralPath $ld -Destination $backup -Force }
    $result = Set-AuthenticodeSignature -FilePath $ld -Certificate $cert -HashAlgorithm SHA256
    if ($result.Status -ne 'Valid') { throw "Signing failed: $($result.StatusMessage)" }
}

& $ld --version | Select-Object -First 1
if ($LASTEXITCODE -ne 0) { throw "Signed linker still cannot run." }
Write-Output "Signed and verified: $ld"
