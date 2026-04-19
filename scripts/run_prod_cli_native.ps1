$ErrorActionPreference = "Stop"

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo is required"
}

if (-not $env:RUST_LOG) {
    $env:RUST_LOG = "late=debug,late_core=debug"
}

Set-Location $RootDir

$cmdArgs = @(
    "run", "-p", "late-cli", "--bin", "late", "--",
    "--ssh-mode", "native",
    "--ssh-target", $(if ($env:LATE_PROD_SSH_TARGET) { $env:LATE_PROD_SSH_TARGET } else { "late.sh" }),
    "--api-base-url", $(if ($env:LATE_PROD_API_BASE_URL) { $env:LATE_PROD_API_BASE_URL } else { "https://api.late.sh" }),
    "--audio-base-url", $(if ($env:LATE_PROD_AUDIO_BASE_URL) { $env:LATE_PROD_AUDIO_BASE_URL } else { "https://audio.late.sh" }),
    "--verbose"
)

if ($env:LATE_PROD_SSH_PORT) {
    $cmdArgs += @("--ssh-port", $env:LATE_PROD_SSH_PORT)
}

if ($env:LATE_PROD_SSH_USER) {
    $cmdArgs += @("--ssh-user", $env:LATE_PROD_SSH_USER)
}

& cargo @cmdArgs @args
exit $LASTEXITCODE
