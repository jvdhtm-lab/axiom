param(
    [string[]]$Path = @("target\release\axiom.exe"),
    [string]$CertThumbprint,
    [switch]$Install
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Resolve-SigningCertificate([string]$Thumbprint) {
    $candidates = if ($Thumbprint) {
        @(Get-Item "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue)
    } else {
        @(Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
            Where-Object { $_.HasPrivateKey -and ($_.Subject -eq 'CN=Axiom Dev Signing' -or $_.Subject -eq 'CN=MeshCloud Dev Signing') })
    }
    if ($candidates.Count -eq 0) {
        throw "No Axiom/MeshCloud code-signing certificate found in Cert:\CurrentUser\My."
    }

    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'CurrentUser')
    $store.Open('ReadOnly')
    foreach ($cert in $candidates) {
        if ($store.Certificates | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }) {
            $store.Close()
            return $cert
        }
    }
    $store.Close()
    $thumbprints = ($candidates | ForEach-Object { $_.Thumbprint }) -join ', '
    throw "None of the candidate certificates ($thumbprints) is trusted in CurrentUser\TrustedPublisher."
}

$cert = Resolve-SigningCertificate $CertThumbprint
$signed = @()
foreach ($item in $Path) {
    $full = if ([IO.Path]::IsPathRooted($item)) { $item } else { Join-Path $root $item }
    if (-not (Test-Path -LiteralPath $full)) { throw "Cannot find $full" }
    $result = Set-AuthenticodeSignature -FilePath $full -Certificate $cert -HashAlgorithm SHA256
    if ($result.Status -ne 'Valid') { throw "Signing $full failed: $($result.StatusMessage)" }
    $signed += $full
    Write-Output "signed: $full"
}

if ($Install) {
    $installDir = Join-Path $env:USERPROFILE '.cargo\bin'
    if (-not (Test-Path -LiteralPath $installDir)) {
        New-Item -ItemType Directory -Path $installDir | Out-Null
    }
    $release = Join-Path $root 'target\release\axiom.exe'
    if (-not (Test-Path -LiteralPath $release)) { throw "Build the release CLI first: $release" }
    $target = Join-Path $installDir 'axiom.exe'
    Copy-Item -LiteralPath $release -Destination $target -Force
    $copy = Get-AuthenticodeSignature -LiteralPath $target
    if ($copy.Status -ne 'Valid') { throw "Installed copy is not validly signed: $($copy.Status)" }
    Write-Output "installed: $target"
}
