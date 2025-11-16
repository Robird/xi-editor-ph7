[CmdletBinding()]
param(
    [string]
    $Filter
)

$ErrorActionPreference = 'Stop'

function Invoke-Step {
    param(
        [string]$Message,
        [string]$Command,
        [string[]]$Arguments = @()
    )

    Write-Host $Message
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed ($Command $($Arguments -join ' ')) with exit code $LASTEXITCODE"
    }
}

$filter = $Filter

if ($filter) {
    Write-Host "Test filter detected: $filter"
}

$filterArgs = @()
if ($filter) {
    $filterArgs += $filter
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $scriptDir
try {
    Invoke-Step "Checking cargo clippy availability" 'cargo' @('clippy', '--version')
    Invoke-Step "Checking cargo fmt availability" 'cargo' @('fmt', '--version')

    Write-Host 'Running rustfmt:'
    Invoke-Step "cargo fmt --all -- --check" 'cargo' @('fmt', '--all', '--', '--check')
    Write-Host 'Rustfmt check passed.'

    Write-Host 'Running clippy:'
    Invoke-Step "cargo clippy --all -- -D warnings" 'cargo' @('clippy', '--all', '--', '-D', 'warnings')
    Write-Host 'Clippy check passed.'

    Write-Host 'Checking compiler warnings:'
    $previousRustflags = $env:RUSTFLAGS
    try {
        $env:RUSTFLAGS = '-D warnings'
        Invoke-Step "RUSTFLAGS='-D warnings' cargo check --all" 'cargo' @('check', '--all')
    }
    finally {
        if ($null -eq $previousRustflags) {
            Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:RUSTFLAGS = $previousRustflags
        }
    }
    Write-Host 'Check passed!'

    Write-Host 'Running "cargo test --all"'
    $testAllArgs = @('test', '--all')
    if ($filterArgs.Count -gt 0) {
        $testAllArgs += $filterArgs
    }
    Invoke-Step 'cargo test --all' 'cargo' $testAllArgs

    Write-Host 'Running "cargo test -p xi-rope --no-default-features"'
    $testNoDefaultArgs = @('test', '-p', 'xi-rope', '--no-default-features')
    if ($filterArgs.Count -gt 0) {
        $testNoDefaultArgs += $filterArgs
    }
    Invoke-Step 'cargo test -p xi-rope --no-default-features' 'cargo' $testNoDefaultArgs

    Write-Host 'Running "cargo test -p xi-rope --features serde"'
    $testSerdeArgs = @('test', '-p', 'xi-rope', '--features', 'serde')
    if ($filterArgs.Count -gt 0) {
        $testSerdeArgs += $filterArgs
    }
    Invoke-Step 'cargo test -p xi-rope --features serde' 'cargo' $testSerdeArgs

    Write-Host 'Tests passed!'
    Write-Host 'Benchmarks are disabled in xi-editor-ph7; skipping cargo bench.'
    Write-Host 'Workspace limited to core crates (xi-core, xi-core-lib, xi-plugin-lib, xi-rope, xi-rpc, xi-trace, xi-unicode).'
    Write-Host 'All checks passed.'
}
finally {
    Pop-Location
}
