[CmdletBinding()]
param(
    [switch]$VerboseInstaller,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# github.com only speaks TLS 1.2 or newer, and Windows PowerShell 5.1 on
# older .NET hosts does not offer TLS 1.2 unless asked. Without this the
# checksum fetch fails with "Could not create SSL/TLS secure channel".
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$LateBinName = "late.exe"
$LateDefaultBaseUrl = "https://cli.late.sh"
# Checksums are read from the GitHub Release, never from the download host:
# a binary served by cli.late.sh must match a sha256sums.txt served by
# GitHub, so swapping files on one origin is not enough to ship a different
# binary. That holds for a trusted copy of this script; the script itself is
# also served from cli.late.sh, so `irm ... | iex` trusts the host for the
# installer. Every release also carries a Sigstore provenance bundle next to
# each binary (<binary>.sigstore.json); see late-cli/README.md to verify it.
$LateDefaultChecksumBaseUrl = "https://github.com/mpiorowski/late-sh/releases/download"

function Write-Log {
    param([Parameter(Mandatory = $true)][string]$Message)
    Write-Host "late installer: $Message"
}

function Write-VerboseLog {
    param([Parameter(Mandatory = $true)][string]$Message)
    if ($VerboseInstaller) {
        Write-Log $Message
    }
}

function Fail {
    param([Parameter(Mandatory = $true)][string]$Message)
    throw "late installer: $Message"
}

function Get-Target {
    $arch = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($arch)) {
        $arch = $env:PROCESSOR_ARCHITECTURE
    }
    if ([string]::IsNullOrWhiteSpace($arch)) {
        Fail "could not determine CPU architecture"
    }

    $arch = $arch.Trim().ToUpperInvariant()
    if ($arch -eq "AMD64" -or $arch -eq "X64" -or $arch -eq "X86_64") {
        return "x86_64-pc-windows-msvc"
    }
    if ($arch -eq "ARM64") {
        Fail "unsupported architecture: ARM64 (native ARM64 build is not published yet)"
    }

    Fail "unsupported architecture: $arch"
}

# Release tags are used verbatim in URL paths on two hosts, so a value that
# came from the network is only accepted if it looks like a tag. Without this
# a tampered VERSION file could point the checksum URL at another GitHub path.
function Test-VersionString {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Version,
        [Parameter(Mandatory = $true)][string]$Source
    )

    if ($Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        Fail "unexpected version string from ${Source}: $Version"
    }
}

# `latest` is a pointer, not a release. It resolves to the tag in
# {base}/latest/VERSION and everything after that (binary, checksums) is
# fetched for that exact tag, so a publish in progress cannot mix files from
# two versions.
function Resolve-Version {
    param(
        [Parameter(Mandatory = $true)][string]$BaseUrl,
        [Parameter(Mandatory = $true)][string]$Requested
    )

    if ($Requested -ne "latest") {
        Test-VersionString -Version $Requested -Source "LATE_INSTALL_VERSION"
        return $Requested
    }

    $versionUrl = "$($BaseUrl.TrimEnd('/'))/latest/VERSION"
    try {
        $body = (Invoke-WebRequest -Uri $versionUrl -UseBasicParsing).Content
    } catch {
        Fail "could not resolve the latest version from ${versionUrl}: $($_.Exception.Message)"
    }

    if ($body -is [byte[]]) {
        $body = [System.Text.Encoding]::UTF8.GetString($body)
    }
    $version = ([string]$body).Trim()
    if ([string]::IsNullOrWhiteSpace($version)) {
        Fail "empty version file at $versionUrl"
    }
    Test-VersionString -Version $version -Source $versionUrl
    return $version
}

function Get-InstallDir {
    if ($env:LATE_INSTALL_DIR) {
        return $env:LATE_INSTALL_DIR
    }

    if (-not $env:LOCALAPPDATA) {
        Fail "LOCALAPPDATA is not set"
    }

    return (Join-Path $env:LOCALAPPDATA "Programs\late")
}

function Get-ExpectedChecksum {
    param(
        [Parameter(Mandatory = $true)][string]$ChecksumFile,
        [Parameter(Mandatory = $true)][string]$Target,
        [Parameter(Mandatory = $true)][string]$BinaryName
    )

    foreach ($line in Get-Content -Path $ChecksumFile) {
        $parts = $line -split '\s+', 3
        if ($parts.Length -ge 2 -and $parts[1] -eq "$Target/$BinaryName") {
            return $parts[0]
        }
    }

    Fail "missing checksum for $Target/$BinaryName"
}

function Test-PathContainsDir {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$PathValue,
        [Parameter(Mandatory = $true)][string]$Directory
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $normalizedDir = [System.IO.Path]::GetFullPath($Directory).TrimEnd('\')

    foreach ($entry in $PathValue.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)) {
        try {
            $normalizedEntry = [System.IO.Path]::GetFullPath($entry).TrimEnd('\')
        } catch {
            $normalizedEntry = $entry.TrimEnd('\')
        }

        if ([string]::Equals($normalizedEntry, $normalizedDir, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

if ($Help) {
    @"
late installer

Options:
  -VerboseInstaller   Print resolved target, URLs, and install paths
  -Help               Show this help

Environment:
  LATE_INSTALL_BASE_URL            Override distribution host
  LATE_INSTALL_CHECKSUM_BASE_URL   Override where sha256sums.txt is read from
                                   (default: the GitHub Release assets)
  LATE_INSTALL_VERSION             Use a specific version instead of latest
  LATE_INSTALL_DIR                 Override the install directory
"@
    exit 0
}

$baseUrl = if ($env:LATE_INSTALL_BASE_URL) { $env:LATE_INSTALL_BASE_URL } else { $LateDefaultBaseUrl }
$checksumBaseUrl = if ($env:LATE_INSTALL_CHECKSUM_BASE_URL) { $env:LATE_INSTALL_CHECKSUM_BASE_URL } else { $LateDefaultChecksumBaseUrl }
$requestedVersion = if ($env:LATE_INSTALL_VERSION) { $env:LATE_INSTALL_VERSION } else { "latest" }
$version = Resolve-Version -BaseUrl $baseUrl -Requested $requestedVersion
$target = Get-Target
$prefix = "releases/$version"
$binaryUrl = "$($baseUrl.TrimEnd('/'))/$prefix/$target/$LateBinName"
$checksumUrl = "$($checksumBaseUrl.TrimEnd('/'))/$version/sha256sums.txt"
$targetDir = Get-InstallDir

Write-VerboseLog "base_url=$baseUrl"
Write-VerboseLog "version=$version"
Write-VerboseLog "target=$target"
Write-VerboseLog "binary_url=$binaryUrl"
Write-VerboseLog "checksum_url=$checksumUrl"
Write-VerboseLog "target_dir=$targetDir"

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    $downloadedBinary = Join-Path $tempDir $LateBinName
    $checksumFile = Join-Path $tempDir "sha256sums.txt"

    # Verification fails closed: an unavailable checksum file, a missing
    # entry, or a mismatch all abort the install.
    Write-Log "fetching checksums for $version from $checksumUrl"
    try {
        Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumFile -UseBasicParsing
    } catch {
        Fail "checksum file unavailable at ${checksumUrl}; refusing to install an unverified binary"
    }

    Write-Log "downloading $target from $binaryUrl"
    Invoke-WebRequest -Uri $binaryUrl -OutFile $downloadedBinary -UseBasicParsing

    $expected = Get-ExpectedChecksum -ChecksumFile $checksumFile -Target $target -BinaryName $LateBinName
    $actual = (Get-FileHash -Algorithm SHA256 -Path $downloadedBinary).Hash.ToLowerInvariant()
    if ($actual -ne $expected.ToLowerInvariant()) {
        Fail "checksum mismatch for $LateBinName"
    }
    Write-VerboseLog "verified sha256 of ${LateBinName}: $actual"

    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
    $destPath = Join-Path $targetDir $LateBinName
    Copy-Item -Path $downloadedBinary -Destination $destPath -Force
    Write-Log "installed $LateBinName to $destPath"

    if (-not (Test-PathContainsDir -PathValue $env:PATH -Directory $targetDir)) {
        Write-Log "warning: $targetDir is not currently on PATH"
        Write-Log "add it with: [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$targetDir', 'User')"
    }

    Write-Log "run '& `"$destPath`" --help' to verify the install"
} finally {
    Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
