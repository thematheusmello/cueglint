# A native, dependency-free release link checks the actual Cargo/MSVC filename contract.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'product-link-maps.ps1')

$triple = 'x86_64-pc-windows-msvc'
if (-not $IsWindows -or (& rustc +1.94.1 -vV) -cnotcontains "host: $triple") {
    throw 'The product wrapper control requires native x64 MSVC.'
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('cueglint wrapper control ' + [Guid]::NewGuid().ToString('N'))
$target = Join-Path $root 'target'
$wrapper = Join-Path $root 'map-wrapper.exe'
$unitTests = Join-Path $root 'map-wrapper-tests.exe'
$source = Join-Path $PSScriptRoot 'product-link-map-wrapper.rs'
$environment = @{}
foreach ($key in @('CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC_WRAPPER', '__CARGO_DEFAULT_LIB_METADATA')) {
    $environment[$key] = [Environment]::GetEnvironmentVariable($key)
}
[void][IO.Directory]::CreateDirectory($target)
try {
    & rustc +1.94.1 --edition=2024 --test $source -o $unitTests
    if ($LASTEXITCODE -ne 0) { throw 'Product wrapper unit tests did not compile.' }
    & $unitTests
    if ($LASTEXITCODE -ne 0) { throw 'Product wrapper unit tests failed.' }
    & rustc +1.94.1 --edition=2024 -D warnings $source -o $wrapper
    if ($LASTEXITCODE -ne 0) { throw 'Product wrapper did not compile.' }

    @'
[workspace]
members = ["cueglint", "cueglint-cli"]
resolver = "3"

[profile.release]
lto = "thin"
codegen-units = 1
'@ | Set-Content -Encoding ascii -LiteralPath (Join-Path $root 'Cargo.toml')
    foreach ($name in @('cueglint', 'cueglint-cli')) {
        $package = Join-Path $root $name
        [void][IO.Directory]::CreateDirectory((Join-Path $package 'src'))
        @"
[package]
name = "$name"
version = "0.0.0"
edition = "2024"
"@ | Set-Content -Encoding ascii -LiteralPath (Join-Path $package 'Cargo.toml')
        "fn main() { println!(""$name original link control""); }" |
            Set-Content -Encoding ascii -LiteralPath (Join-Path $package 'src/main.rs')
    }

    $cargoOutput = Join-Path $root 'build.jsonl'
    $cargoStderr = Join-Path $root 'build.stderr.txt'
    Push-Location -LiteralPath $root
    try {
        $env:CARGO_TARGET_DIR = $target
        $env:CARGO_BUILD_BUILD_DIR = $null
        $env:RUSTC_WORKSPACE_WRAPPER = $wrapper
        $env:RUSTC_WRAPPER = ''
        $env:__CARGO_DEFAULT_LIB_METADATA = $null
        & cargo +1.94.1 generate-lockfile --offline
        if ($LASTEXITCODE -ne 0) { throw 'Original workspace lockfile failed.' }
        & cargo +1.94.1 build --offline --locked --release --message-format=json --target $triple -p cueglint -p cueglint-cli > $cargoOutput 2> $cargoStderr
        $buildExit = $LASTEXITCODE
        if ($buildExit -ne 0) {
            $reason = @(Get-ProductLinkWrapperRefusals $cargoStderr) -join ', '
            throw "Original workspace release build failed with exit $buildExit. $reason"
        }
    } finally { Pop-Location }

    $artifacts = @(Get-Content -LiteralPath $cargoOutput | ConvertFrom-Json |
        Where-Object { $_.reason -ceq 'compiler-artifact' -and $_.executable })
    $report = Get-ProductLinkMapReport $target $artifacts
    if ($report.status -cne 'observed' -or $report.binaries.Count -ne 2) {
        throw "Original workspace maps were refused ($($report.reason))."
    }
    foreach ($name in @('cueglint', 'cueglint-cli')) {
        $marker = Join-Path $target "cueglint-link-maps/$name.linked"
        if ((Get-Content -LiteralPath $marker -Raw).TrimEnd([char]13, [char]10) -cne $name.Replace('-', '_')) {
            throw 'The MSVC workspace control did not exercise the plain Cargo stem.'
        }
    }
    Write-Output 'Native MSVC workspace control verified both plain-stem PE maps.'
} finally {
    foreach ($key in $environment.Keys) { [Environment]::SetEnvironmentVariable($key, $environment[$key]) }
    Remove-Item -LiteralPath $root -Force -Recurse
}
