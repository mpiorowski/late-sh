[CmdletBinding()]
param(
    [switch]$VerboseInstaller,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$LateBinName = "late.exe"
$LateDefaultBaseUrl = "https://cli.late.sh"

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
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

    switch ($arch) {
        "X64" {
            return "x86_64-pc-windows-msvc"
        }
        "Arm64" {
            Fail "unsupported architecture: ARM64 (native ARM64 build is not published yet)"
        }
        default {
            Fail "unsupported architecture: $arch"
        }
    }
}

function Get-Prefix {
    param([Parameter(Mandatory = $true)][string]$Version)

    if ($Version -eq "latest") {
        return "latest"
    }

    return "releases/$Version"
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
  LATE_INSTALL_BASE_URL   Override distribution host
  LATE_INSTALL_VERSION    Use a specific version instead of latest
  LATE_INSTALL_DIR        Override the install directory
"@
    exit 0
}

$baseUrl = if ($env:LATE_INSTALL_BASE_URL) { $env:LATE_INSTALL_BASE_URL } else { $LateDefaultBaseUrl }
$version = if ($env:LATE_INSTALL_VERSION) { $env:LATE_INSTALL_VERSION } else { "latest" }
$target = Get-Target
$prefix = Get-Prefix -Version $version
$binaryUrl = "$($baseUrl.TrimEnd('/'))/$prefix/$target/$LateBinName"
$checksumUrl = "$($baseUrl.TrimEnd('/'))/$prefix/sha256sums.txt"
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

    Write-Log "downloading $target from $binaryUrl"
    Invoke-WebRequest -Uri $binaryUrl -OutFile $downloadedBinary

    try {
        Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumFile
        $expected = Get-ExpectedChecksum -ChecksumFile $checksumFile -Target $target -BinaryName $LateBinName
        $actual = (Get-FileHash -Algorithm SHA256 -Path $downloadedBinary).Hash.ToLowerInvariant()
        if ($actual -ne $expected.ToLowerInvariant()) {
            Fail "checksum mismatch for $LateBinName"
        }
    } catch {
        if ($_.Exception.Message -like "late installer:*") {
            throw
        }

        Write-Log "warning: checksum file unavailable at $checksumUrl; continuing without verification"
    }

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
