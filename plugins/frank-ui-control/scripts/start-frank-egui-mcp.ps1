[CmdletBinding()]
param(
    [string]$Repository = $env:FRANK_REPOSITORY
)

$ErrorActionPreference = "Stop"

function Test-FrankRepository {
    param([Parameter(Mandatory)][string]$Path)

    return (Test-Path -LiteralPath (Join-Path $Path "compose.yaml")) -and
        (Test-Path -LiteralPath (Join-Path $Path "docker/Dockerfile.ui-test"))
}

function Find-FrankRepository {
    param([string]$ExplicitPath)

    if ($ExplicitPath) {
        $resolved = (Resolve-Path -LiteralPath $ExplicitPath).Path
        if (-not (Test-FrankRepository -Path $resolved)) {
            throw "FRANK_REPOSITORY does not point to a Frank checkout: $resolved"
        }
        return $resolved
    }

    $cursor = (Get-Location).Path
    while ($cursor) {
        if (Test-FrankRepository -Path $cursor) {
            return $cursor
        }
        $parent = Split-Path -Parent $cursor
        if ($parent -eq $cursor) {
            break
        }
        $cursor = $parent
    }

    throw "Frank repository not found. Open Codex in the cloned repository or set FRANK_REPOSITORY."
}

function Invoke-DockerCapture {
    param([Parameter(Mandatory)][string[]]$Arguments)

    # Windows PowerShell wraps any native stderr line in an ErrorRecord when the
    # caller uses Stop. Compose emits normal build progress on stderr, so rely on
    # the native process exit code and retain the combined output for failures.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & docker @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) {
        throw "docker $($Arguments -join ' ') failed:`n$($output -join [Environment]::NewLine)"
    }
    return $output
}

$repositoryRoot = Find-FrankRepository -ExplicitPath $Repository

# Keep MCP stdout clean: only the final egui-mcp process may write protocol data there.
Invoke-DockerCapture -Arguments @(
    "compose", "--project-directory", $repositoryRoot,
    "up", "--build", "-d", "ui-test"
) | Out-Null

$containerId = (Invoke-DockerCapture -Arguments @(
    "compose", "--project-directory", $repositoryRoot,
    "ps", "-q", "ui-test"
) | Select-Object -First 1).Trim()

if (-not $containerId) {
    throw "The Frank ui-test container did not start."
}

$deadline = (Get-Date).AddMinutes(3)
do {
    $state = (Invoke-DockerCapture -Arguments @(
        "inspect", "--format",
        "{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}",
        $containerId
    ) | Select-Object -First 1).Trim()

    if ($state -eq "healthy") {
        break
    }
    if ($state -in @("dead", "exited")) {
        throw "The Frank ui-test container stopped before becoming healthy."
    }
    if ((Get-Date) -ge $deadline) {
        throw "Timed out waiting for the Frank ui-test container to become healthy (last state: $state)."
    }
    Start-Sleep -Seconds 1
} while ($true)

& docker compose --project-directory $repositoryRoot exec -T ui-test egui-mcp
exit $LASTEXITCODE
