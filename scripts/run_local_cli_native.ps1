$ErrorActionPreference = "Stop"

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Get-EnvOrDefault {
    param(
        [Parameter(Mandatory = $true)][string]$Key,
        [Parameter(Mandatory = $true)][string]$Fallback
    )

    $envFile = Join-Path $RootDir ".env"
    if (Test-Path $envFile) {
        $match = Select-String -Path $envFile -Pattern "^$([regex]::Escape($Key))=(.*)$" | Select-Object -Last 1
        if ($match) {
            $value = $match.Matches[0].Groups[1].Value.Trim()
            if (-not [string]::IsNullOrWhiteSpace($value)) {
                return $value
            }
        }
    }

    return $Fallback
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo is required"
}

$sshPort = if ($env:LATE_LOCAL_SSH_PORT) { $env:LATE_LOCAL_SSH_PORT } else { Get-EnvOrDefault "LATE_SSH_PORT" "2222" }
$apiPort = Get-EnvOrDefault "LATE_API_PORT" "4000"
$webPort = Get-EnvOrDefault "LATE_WEB_PORT" "3000"
$apiBaseUrl = if ($env:LATE_LOCAL_API_BASE_URL) { $env:LATE_LOCAL_API_BASE_URL } else { "http://localhost:$apiPort" }
$audioBaseUrl = if ($env:LATE_LOCAL_AUDIO_BASE_URL) { $env:LATE_LOCAL_AUDIO_BASE_URL } else { "http://localhost:$webPort/stream" }
$sshTarget = if ($env:LATE_LOCAL_SSH_TARGET) { $env:LATE_LOCAL_SSH_TARGET } else { "localhost" }

Set-Location $RootDir

$cmdArgs = @(
    "run", "-p", "late-cli", "--bin", "late", "--",
    "--ssh-mode", "native",
    "--ssh-target", $sshTarget,
    "--ssh-port", $sshPort,
    "--api-base-url", $apiBaseUrl,
    "--audio-base-url", $audioBaseUrl
)

if ($env:LATE_LOCAL_SSH_USER) {
    $cmdArgs += @("--ssh-user", $env:LATE_LOCAL_SSH_USER)
}

& cargo @cmdArgs @args
exit $LASTEXITCODE
