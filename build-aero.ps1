<#
.SYNOPSIS
  Builds the Aero node (aerod) and the RandomX miner (aero-miner) on Windows with the GNU toolchain.

.DESCRIPTION
  This repo builds with the GNU (MinGW) Rust toolchain, because no MSVC linker is installed. The script:
    1. checks the prerequisites and says exactly what is missing and how to install it,
    2. sets the environment (separate target dir, Ninja for the RandomX C++ build, LLVM and protoc paths),
    3. builds in release mode (optionally runs the tests first).

.PARAMETER Test
  Run the test suite for every crate that can build here before building the binaries.

.PARAMETER Debug
  Build a debug profile instead of release (RandomX itself is always built optimized).

.PARAMETER NoBuild
  Only check the prerequisites and set up the environment; do not build.

.EXAMPLE
  .\build-aero.ps1
  .\build-aero.ps1 -Test
#>
param(
    [switch]$Test,
    [switch]$Debug,
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$Toolchain = "stable-x86_64-pc-windows-gnu"
$TargetDir = Join-Path $PSScriptRoot "target-gnu"
$Profile   = if ($Debug) { "debug" } else { "release" }

function Fail($message) { Write-Host "`nERROR: $message" -ForegroundColor Red; exit 1 }
function Ok($message)   { Write-Host "  [ok] $message" -ForegroundColor Green }
function Find-Tool($name) { $c = Get-Command $name -ErrorAction SilentlyContinue; if ($c) { $c.Source } else { $null } }

Write-Host "== Checking prerequisites ==" -ForegroundColor Cyan

# --- Rust GNU toolchain -------------------------------------------------------------------------------------------
if (-not (Find-Tool "rustup")) { Fail "rustup not found. Install Rust from https://rustup.rs" }
if (-not ((rustup toolchain list) -match [regex]::Escape($Toolchain))) {
    Fail "Rust toolchain '$Toolchain' is not installed. Run: rustup toolchain install $Toolchain"
}
Ok "Rust toolchain $Toolchain"

# --- C/C++ toolchain (MinGW) for the RandomX build and the GNU linker -------------------------------------------------
foreach ($t in "gcc", "g++") {
    $p = Find-Tool $t
    if (-not $p) { Fail "'$t' not found on PATH. Install MinGW-w64 (e.g. 'winget install BrechtSanders.WinLibs.POSIX.UCRT')." }
    Ok "$t -> $p"
}

# --- CMake + Ninja (RandomX is built with CMake) ----------------------------------------------------------------------
foreach ($t in "cmake", "ninja") {
    $p = Find-Tool $t
    if (-not $p) { Fail "'$t' not found on PATH. Install it (the WinLibs MinGW package above includes both, or 'winget install Kitware.CMake Ninja-build.Ninja')." }
    Ok "$t -> $p"
}

# --- LLVM / libclang (RocksDB's bindgen needs it) ---------------------------------------------------------------------
if (-not $env:LIBCLANG_PATH) {
    foreach ($candidate in @("C:\Program Files\LLVM\bin", "C:\Program Files (x86)\LLVM\bin")) {
        if (Test-Path (Join-Path $candidate "libclang.dll")) { $env:LIBCLANG_PATH = $candidate; break }
    }
}
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH "libclang.dll"))) {
    Fail "libclang.dll not found. Install LLVM ('winget install LLVM.LLVM') or set LIBCLANG_PATH to the folder containing libclang.dll."
}
Ok "LIBCLANG_PATH = $env:LIBCLANG_PATH"

# RocksDB's bindgen needs clang's builtin headers (stdbool.h...) and MinGW's system headers.
$clangRoot = Join-Path (Split-Path $env:LIBCLANG_PATH -Parent) "lib\clang"
$clangInclude = $null
if (Test-Path $clangRoot) {
    $dir = Get-ChildItem $clangRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($dir) { $clangInclude = Join-Path $dir.FullName "include" }
}
if (-not $clangInclude -or -not (Test-Path (Join-Path $clangInclude "stdbool.h"))) {
    Fail "Could not find clang's builtin headers under $clangRoot (looked for lib\clang\<version>\include\stdbool.h)."
}
$mingwInclude = Join-Path (Split-Path (Split-Path (Find-Tool "gcc") -Parent) -Parent) "x86_64-w64-mingw32\include"
$env:BINDGEN_EXTRA_CLANG_ARGS = ('-I"{0}" -I"{1}"' -f ($clangInclude -replace '\\', '/'), ($mingwInclude -replace '\\', '/'))
Ok "BINDGEN_EXTRA_CLANG_ARGS = $env:BINDGEN_EXTRA_CLANG_ARGS"

# --- protoc (gRPC / p2p protobuf code generation) ---------------------------------------------------------------------
if (-not $env:PROTOC) {
    $protoc = Find-Tool "protoc"
    if ($protoc) { $env:PROTOC = $protoc }
}
if (-not $env:PROTOC -or -not (Test-Path $env:PROTOC)) {
    Fail "protoc not found. Download protoc-*-win64.zip from https://github.com/protocolbuffers/protobuf/releases, put protoc.exe on PATH, or set PROTOC to its full path."
}
Ok "PROTOC = $env:PROTOC"

# --- Environment for the build ----------------------------------------------------------------------------------------
$env:CARGO_TARGET_DIR = $TargetDir
$env:CMAKE_GENERATOR  = "Ninja"
Ok "CARGO_TARGET_DIR = $TargetDir"
Ok "CMAKE_GENERATOR  = Ninja"

if ($NoBuild) { Write-Host "`nPrerequisites OK (-NoBuild given, stopping here)." -ForegroundColor Cyan; exit 0 }

$cargo = @("+$Toolchain")
$profileArgs = if ($Debug) { @() } else { @("--release") }

# --- Tests (optional) ---------------------------------------------------------------------------------------------------
if ($Test) {
    Write-Host "`n== Running tests ==" -ForegroundColor Cyan
    & cargo @cargo test --workspace --no-fail-fast
    if ($LASTEXITCODE -ne 0) { Fail "Some tests failed (see above). Fix them or build without -Test." }
}

# --- Build --------------------------------------------------------------------------------------------------------------
Write-Host "`n== Building aerod, aero-miner and aero-core ($Profile) ==" -ForegroundColor Cyan
& cargo @cargo build @profileArgs -p aerod -p aero-miner -p aero-core-gui
if ($LASTEXITCODE -ne 0) { Fail "Build failed (see the compiler errors above)." }

$bin = Join-Path $TargetDir $Profile
Write-Host "`n== Done ==" -ForegroundColor Green
Write-Host "  Node  : $(Join-Path $bin 'aerod.exe')"
Write-Host "  Miner : $(Join-Path $bin 'aero-miner.exe')"
Write-Host "  App   : $(Join-Path $bin 'aero-core.exe')   (the GUI node + wallet; keep it next to aerod.exe)"
Write-Host @"

Quick start on a private devnet:
  1. $bin\aero-core.exe --devnet     (starts its own node; copy an address from the Receive tab)
  2. $bin\aero-miner.exe -s 127.0.0.1:16610 -a <that-address> --mine-unsynced
  The Overview tab fills in as blocks arrive. Add --light to the miner on machines with under ~2.5 GiB free RAM.

Notes:
  * Drop --light on a machine with ~2.5 GiB free RAM for ~9x faster mining.
  * To validate headers in RandomX fast mode on a server, set AERO_RANDOMX_FAST_VALIDATION=1 for aerod.
"@
