<#
.SYNOPSIS
    LyraCore Windows setup - the counterpart of install.sh.

.DESCRIPTION
    Checks the prerequisites for running the LyraCore development realm on Windows, installs the
    ones it can user-locally (Rust, SpacetimeDB exactly 2.7.1, wasm-opt), builds the gateway and the
    vendored Windows CLI, and prints what to run next.

    It NEVER runs an elevated command and NEVER touches the network state of anything already
    installed: a present tool is left exactly as it is. Run it from the checkout root:

        powershell -ExecutionPolicy Bypass -File .\install.ps1

    This is the Windows sibling of the POSIX `install.sh`. Where install.sh clones the repository,
    this assumes the checkout is already on disk (you are inside it) - the Windows flow starts from
    a folder, not a `curl | sh` pipe.

    ASCII-only on purpose: Windows PowerShell 5.1 reads a .ps1 without a BOM as ANSI, so a box-
    drawing check mark in the source would break the parser.

.PARAMETER Yes
    Assume yes to every prompt. A prompt is only ever shown before downloading and running a
    third-party installer.

.PARAMETER SkipTools
    Only check prerequisites and build; do not offer to install anything.
#>
[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$SkipTools
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$SpacetimeVersion = '2.7.1'
$BinaryenVersion = '131'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

function Say($text) { Write-Host $text }
function Note($text) { Write-Host "  $text" }
function Good($text) { Write-Host "  [ok] $text" -ForegroundColor Green }
function Warn($text) { Write-Host "  [warn] $text" -ForegroundColor Yellow }
function Fail($text) { Write-Host "install.ps1: $text" -ForegroundColor Red; exit 1 }

function Have($command) {
    return [bool](Get-Command $command -ErrorAction SilentlyContinue)
}

function Confirm-Step($question) {
    if ($Yes) { Write-Host "$question [y/N] y (-Yes)"; return $true }
    $reply = Read-Host "$question [y/N]"
    return $reply -match '^(y|yes)$'
}

# --------------------------------------------------------------------------------------------
# 0. Layout
# --------------------------------------------------------------------------------------------
if (-not (Test-Path (Join-Path $Root 'Cargo.toml')) -or
    -not (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '\[workspace\]' -Quiet)) {
    Fail "$Root does not look like a LyraCore checkout (no workspace Cargo.toml). Run this from the checkout root."
}
Say 'LyraCore Windows setup - checking prerequisites (nothing here needs elevation).'

# --------------------------------------------------------------------------------------------
# 1. Rust (rustup)
# --------------------------------------------------------------------------------------------
if (Have 'cargo') {
    $cargoVersion = (& cargo --version) -replace 'cargo ', ''
    Good "Rust ($cargoVersion)"
} else {
    if ($SkipTools) { Fail 'cargo is required. Install Rust from https://rustup.rs/ and re-run.' }
    Say ''
    Say 'Rust is not installed. The official rustup installer would be downloaded and run:'
    Note 'https://win.rustup.rs/x86_64'
    if (-not (Confirm-Step 'Install Rust (rustup) now?')) {
        Fail 'Rust is required. Install it from https://rustup.rs/ and re-run this script.'
    }
    $rustup = Join-Path $env:TEMP 'rustup-init.exe'
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $rustup -UseBasicParsing
    & $rustup -y
    if ($LASTEXITCODE -ne 0) { Fail 'the rustup installer failed' }
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    if (-not (Have 'cargo')) { Fail 'rustup installed, but cargo is still not on PATH - open a new shell and re-run' }
    Good "Rust ($((& cargo --version) -replace 'cargo ', ''))"
}

# --------------------------------------------------------------------------------------------
# 2. SpacetimeDB exactly 2.7.1
# --------------------------------------------------------------------------------------------
if (-not (Have 'spacetime')) {
    if ($SkipTools) { Fail 'SpacetimeDB is required. Install it and re-run.' }
    Say ''
    Say 'SpacetimeDB is not installed. The official Windows installer would be downloaded and run:'
    Note 'https://windows.spacetimedb.com'
    if (-not (Confirm-Step 'Install SpacetimeDB now?')) {
        Fail 'SpacetimeDB is required. Install it and re-run this script.'
    }
    $installer = Join-Path $env:TEMP 'stdb-install.ps1'
    Invoke-WebRequest 'https://windows.spacetimedb.com' -OutFile $installer -UseBasicParsing
    & powershell -ExecutionPolicy Bypass -File $installer
    $env:Path = "$env:LOCALAPPDATA\SpacetimeDB;$env:Path"
    if (-not (Have 'spacetime')) { Fail 'SpacetimeDB installed, but `spacetime` is still not on PATH - open a new shell and re-run' }
}

$found = (& spacetime --version 2>&1 | Select-String -Pattern 'tool version ([0-9][0-9.]*)' |
    ForEach-Object { $_.Matches[0].Groups[1].Value } | Select-Object -First 1)
if ($found -eq $SpacetimeVersion) {
    Good "SpacetimeDB $found"
} else {
    if ($SkipTools) { Fail "SpacetimeDB $SpacetimeVersion is required (found: $found)." }
    $foundText = if ($found) { $found } else { 'unknown' }
    Say ''
    Say "This repository is pinned to SpacetimeDB exactly $SpacetimeVersion (found: $foundText)."
    Say 'Versions install side by side, so this is not destructive.'
    if (-not (Confirm-Step "Pin SpacetimeDB to $SpacetimeVersion now?")) {
        Fail "SpacetimeDB $SpacetimeVersion is required (`lyracore preflight` enforces an exact match)."
    }
    & spacetime version install $SpacetimeVersion
    & spacetime version use $SpacetimeVersion
    Good "SpacetimeDB $SpacetimeVersion"
}

# --------------------------------------------------------------------------------------------
# 3. wasm-opt (Binaryen) - optional; `spacetime publish` optimises the module with it
# --------------------------------------------------------------------------------------------
if (Have 'wasm-opt') {
    Good "wasm-opt ($(& wasm-opt --version))"
} elseif ($SkipTools) {
    Warn 'wasm-opt not found - publish still works, unoptimised'
} else {
    $binaryenDir = "$env:USERPROFILE\.local\share\lyracore\binaryen-version_$BinaryenVersion"
    $binaryenBin = Join-Path $binaryenDir 'bin'
    $url = "https://github.com/WebAssembly/binaryen/releases/download/version_$BinaryenVersion/binaryen-version_$BinaryenVersion-x86_64-windows.tar.gz"
    Say ''
    Say 'wasm-opt is not installed. `spacetime publish` uses it to optimise the module wasm - it still'
    Say 'works without it, just unoptimised. This would be downloaded and extracted, user-local:'
    Note $url
    Note "-> $binaryenDir"
    if (Confirm-Step 'Install wasm-opt now?') {
        $tar = Join-Path $env:TEMP "binaryen-$BinaryenVersion.tar.gz"
        try {
            Invoke-WebRequest $url -OutFile $tar -UseBasicParsing
            New-Item -ItemType Directory -Path "$env:USERPROFILE\.local\share\lyracore" -Force | Out-Null
            tar -xzf $tar -C "$env:USERPROFILE\.local\share\lyracore"
            $env:Path = "$binaryenBin;$env:Path"
            $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
            if ($userPath -notlike "*$binaryenBin*") {
                [Environment]::SetEnvironmentVariable('Path', "$userPath;$binaryenBin", 'User')
            }
            if (Have 'wasm-opt') { Good "wasm-opt ($(& wasm-opt --version))" }
            else { Warn 'wasm-opt was extracted but is not runnable here - continuing without it' }
        } catch {
            Warn "download or extract failed - continuing without wasm-opt ($_)"
        }
    } else {
        Warn 'skipping - publish still works, just unoptimised'
    }
}

# --------------------------------------------------------------------------------------------
# 4. Build the gateway and the vendored Windows CLI
# --------------------------------------------------------------------------------------------
Say ''
Say '- building the gateway...'
& cargo build -p lyracore-gateway
if ($LASTEXITCODE -ne 0) { Fail 'the gateway did not build' }
Good 'gateway built (target\debug\lyracore-gateway.exe)'

Say '- building the Windows CLI...'
& cargo build --manifest-path (Join-Path $Root 'tools\lyracore-cli\Cargo.toml')
if ($LASTEXITCODE -ne 0) { Fail 'the Windows CLI did not build' }
Good 'CLI built (tools\lyracore-cli\target\debug\lyracore.exe)'

# --------------------------------------------------------------------------------------------
# 5. Optional: a launcher on PATH, so `lyracore` works from any subdirectory of any checkout
# --------------------------------------------------------------------------------------------
$binDir = Join-Path $env:USERPROFILE '.local\bin'
$launcher = Join-Path $binDir 'lyracore.cmd'
Say ''
if (Have 'lyracore') {
    Good 'lyracore launcher already on PATH'
} elseif (Confirm-Step "Install a lyracore launcher into $binDir and add it to your PATH?") {
    New-Item -ItemType Directory -Path $binDir -Force | Out-Null
    Copy-Item (Join-Path $Root 'lyracore.cmd') $launcher -Force
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$binDir*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$binDir", 'User')
    }
    Good "launcher installed at $launcher (start a new shell to use it)"
} else {
    Warn 'no launcher installed - use .\lyracore.cmd from the checkout'
}

# --------------------------------------------------------------------------------------------
# 6. Next steps
# --------------------------------------------------------------------------------------------
Say ''
Say "[ok] LyraCore is ready in $Root"
Say ''
Say '  .\lyracore.cmd doctor        # confirm everything is ready'
Say '  .\lyracore.cmd dev up        # start the seeded local realm'
Say '  .\lyracore.cmd account create TEST'
Say ''
Say 'Then point your 1.12.1 (build 5875) client at 127.0.0.1 and log in.'
Say 'See docs\quickstart-windows.md for the full walkthrough.'
