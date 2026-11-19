# Run after the qualification controller's vcvarsall selection. Raw data stays nested.
param([Parameter(Mandatory)][string]$Out,
      [Parameter(Mandatory)][string]$VcRoot)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

function Test-LinkCalibrationTrace([string]$Diagnostics, [string]$Archive) {
    $records = @($Diagnostics -split '\r?\n' | Where-Object { $_.Trim() } |
        ForEach-Object { ConvertFrom-Json -InputObject $_ } |
        Where-Object { $_.code.code -ceq 'linker_messages' -and $_.level -ceq 'warning' -and
            $_.message -is [string] -and $_.message.StartsWith('linker stdout: ', [StringComparison]::Ordinal) })
    if ($records.Count -ne 1) { return $false }
    $trace = $records[0].message.Substring('linker stdout: '.Length)
    $alias = 'cueglint_calibration.lib'
    $search = $false
    $owned = $false
    $found = $false
    $complete = 0
    $selected = 0
    foreach ($line in $trace -split '\r?\n') {
        $line = $line.Trim()
        if ($line -ceq 'Searching libraries') {
            if ($search) { return $false }
            $search = $true
        } elseif ($line -ceq 'Finished searching libraries') {
            if (-not $search -or $found) { return $false }
            $search = $false
            $owned = $false
            $complete++
        } elseif ($line -cmatch '^Searching (.+):$') {
            if ($found) { return $false }
            $path = $Matches[1].Trim('"')
            $owned = $search -and ($path -ieq $Archive)
            if (-not $owned -and $path -imatch '(?:^|[\\/])cueglint_calibration\.lib$') { return $false }
        } elseif ($line -ceq 'Found cueglint_calibration_used') {
            if (-not $owned -or $found) { return $false }
            $found = $true
        } elseif ($line -ceq 'Found cueglint_calibration_unused') {
            return $false
        } elseif ($line.StartsWith('Loaded ', [StringComparison]::Ordinal) -and
                  ($owned -or $line.IndexOf($alias, [StringComparison]::OrdinalIgnoreCase) -ge 0)) {
            if (-not $owned -or -not $found -or $line -cnotmatch '^Loaded (.+)\(([^()]+)\)$') { return $false }
            $library = $Matches[1].Trim('"')
            if (($library -ine $alias -and $library -ine $Archive) -or $Matches[2] -ine 'used.obj') { return $false }
            $found = $false
            $selected++
        } elseif ($owned -and $line -and -not $line.StartsWith('Referenced in ', [StringComparison]::Ordinal)) {
            return $false
        }
    }
    return (-not $search -and $complete -gt 0 -and $selected -eq 1)
}

function Get-LinkCalibrationIdentity([string]$Path, [switch]$Amd64Pe) {
    $file = Get-Item -LiteralPath $Path
    if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $file.Length -le 0 -or $file.Length -gt 8MB) { throw 'invalid_binary_identity' }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -ne $file.Length) { throw 'binary_size_changed' }
        if ($Amd64Pe) {
            $reader = [IO.BinaryReader]::new($stream, [Text.Encoding]::ASCII, $true)
            try {
                if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'missing_mz_header' }
                $stream.Position = 0x3c
                $offset = $reader.ReadInt32()
                if ($offset -lt 64 -or $offset -gt $stream.Length - 26) { throw 'invalid_pe_header' }
                $stream.Position = $offset
                if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'expected_amd64' }
                $stream.Position = $offset + 24
                if ($reader.ReadUInt16() -ne 0x20b) { throw 'expected_pe32_plus' }
                $stream.Position = 0
            } finally { $reader.Dispose() }
        }
        $hash = [Security.Cryptography.SHA256]::HashData($stream)
        if ($stream.Position -ne $file.Length -or $stream.Length -ne $file.Length) { throw 'binary_size_changed' }
        return [ordered]@{ bytes = $file.Length; sha256 = [Convert]::ToHexString($hash).ToLowerInvariant() }
    } finally { $stream.Dispose() }
}

function Invoke-LinkCalibrationTool([string]$Name, [string]$Exe, [string[]]$Arguments) {
    $stdout = Join-Path $private "$Name.stdout"
    $stderr = Join-Path $private "$Name.stderr"
    & $Exe @Arguments 1> $stdout 2> $stderr
    $state.stages[$Name] = $LASTEXITCODE
    foreach ($path in @($stdout, $stderr)) {
        if ((Get-Item -LiteralPath $path).Length -gt 1MB) { throw 'tool_log_limit' }
    }
    if ($state.stages[$Name] -ne 0) { throw 'tool_nonzero' }
}

if (-not $IsWindows -or $env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_OS -cne 'Windows' -or
    $env:RUNNER_ARCH -cne 'X64' -or $env:GITHUB_EVENT_NAME -cne 'workflow_dispatch' -or
    $env:GITHUB_REF -cne 'refs/heads/main') { throw 'Windows hosted qualification is required.' }
$Out = [IO.Path]::GetFullPath($Out)
$tempRoot = [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetFullPath($env:RUNNER_TEMP))
if (-not $Out.StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Use the qualification evidence directory.' }
$private = Join-Path $Out 'link-calibration-raw'
$report = Join-Path $Out 'link-calibration.json'
if ((Get-Item -LiteralPath $Out).Attributes -band [IO.FileAttributes]::ReparsePoint -or
    (Test-Path -LiteralPath $private) -or (Test-Path -LiteralPath $report)) { throw 'Use fresh calibration evidence.' }
[void][IO.Directory]::CreateDirectory($private)

$state = [ordered]@{ schema_version = 1; status = 'refused'; stages = [ordered]@{} }
$phase = 'tool_selection'
Push-Location -LiteralPath $private
try {
    $env:TEMP = $private
    $env:TMP = $private
    $VcRoot = [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetFullPath($VcRoot))
    if (-not $env:VCToolsInstallDir -or $env:VCToolsInstallDir.TrimEnd('\') -ine $VcRoot -or
        $env:WindowsSDKVersion.TrimEnd('\') -cne '10.0.26100.0' -or
        $env:VCToolsVersion -cnotmatch '^[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,8}$') { throw 'expected_pinned_vcvars' }
    foreach ($name in @('CL', '_CL_', 'LINK', '_LINK_', 'link_repro', 'RUSTC_LOG')) {
        if ([Environment]::GetEnvironmentVariable($name)) { throw 'ambient_tool_flags' }
    }
    $tools = Join-Path $VcRoot 'bin\Hostx64\x64'
    $cl = Join-Path $tools 'cl.exe'
    $lib = Join-Path $tools 'lib.exe'
    $link = Join-Path $tools 'link.exe'
    $state['sdk'] = '10.0.26100.0'
    $state['msvc'] = $env:VCToolsVersion
    $state['linker'] = Get-LinkCalibrationIdentity $link
    $state.linker['file'] = 'link.exe'
    Invoke-LinkCalibrationTool 'rustc-version' 'rustc' @('+1.94.1', '-vV')
    $version = Get-Content -LiteralPath 'rustc-version.stdout' -Raw
    if ($version -cnotmatch '(?m)^commit-hash: e408947bfd200af42db322daf0fadfe7e26d3bd1\r?$' -or
        $version -cnotmatch '(?m)^host: x86_64-pc-windows-msvc\r?$') { throw 'expected_pinned_rustc' }
    $state['rustc'] = '1.94.1'

    $phase = 'archive_build'
    Set-Content -LiteralPath 'used.c' -Encoding ascii -Value 'int cueglint_calibration_used(void) { return 23; }'
    Set-Content -LiteralPath 'unused.c' -Encoding ascii -Value 'int cueglint_calibration_unused(void) { return 29; }'
    Set-Content -LiteralPath 'main.rs' -Encoding ascii -Value @'
unsafe extern "C" { fn cueglint_calibration_used() -> i32; }
fn main() { assert_eq!(unsafe { cueglint_calibration_used() }, 23); }
'@
    Invoke-LinkCalibrationTool 'compile' $cl @('/nologo', '/c', '/Zl', 'used.c', 'unused.c')
    $archive = Join-Path $private 'cueglint_calibration.lib'
    Invoke-LinkCalibrationTool 'archive' $lib @('/nologo', '/OUT:cueglint_calibration.lib', 'used.obj', 'unused.obj')
    Invoke-LinkCalibrationTool 'archive-list' $lib @('/nologo', '/LIST', 'cueglint_calibration.lib')
    $members = @(Get-Content -LiteralPath 'archive-list.stdout' | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Sort-Object)
    if (($members -join '|') -cne 'unused.obj|used.obj') { throw 'unexpected_archive_members' }
    $state['archive'] = Get-LinkCalibrationIdentity $archive
    $state.archive['file'] = 'cueglint_calibration.lib'

    $phase = 'link'
    $exe = Join-Path $private 'cueglint_calibration.exe'
    Invoke-LinkCalibrationTool 'link' 'rustc' @('+1.94.1', '--target', 'x86_64-pc-windows-msvc',
        '--edition=2024', '--error-format=json', '-W', 'linker-messages', '-C', "linker=$link",
        '-C', 'link-arg=/VERBOSE:LIB', '-C', "link-arg=$archive", '-o', $exe, 'main.rs')
    $state['executable'] = Get-LinkCalibrationIdentity $exe -Amd64Pe
    $state.executable['file'] = 'cueglint_calibration.exe'
    $state.executable['PE'] = 'AMD64 PE32+'
    if ((Get-LinkCalibrationIdentity $archive).sha256 -cne $state.archive.sha256 -or
        (Get-LinkCalibrationIdentity $link).sha256 -cne $state.linker.sha256) { throw 'link_input_changed' }
    $phase = 'member_trace'
    $state['diagnostics'] = Get-LinkCalibrationIdentity (Join-Path $private 'link.stderr')
    $diagnostics = Get-Content -LiteralPath 'link.stderr' -Raw
    $state['trace_markers'] = [ordered]@{
        linker_messages = $diagnostics.Contains('"linker_messages"')
        linker_stdout = $diagnostics.Contains('linker stdout: ')
        search_finished = $diagnostics.Contains('Finished searching libraries')
        archive_named = $diagnostics.Contains('cueglint_calibration.lib')
        used_symbol_found = $diagnostics.Contains('Found cueglint_calibration_used')
        used_member_named = $diagnostics.Contains('(used.obj)')
    }
    if (-not (Test-LinkCalibrationTrace $diagnostics $archive)) { throw 'unconfirmed_member_selection' }
    $state['selected_members'] = @('used.obj')
    $state['unselected_members'] = @('unused.obj')
    $phase = 'run_control'
    Invoke-LinkCalibrationTool 'execute' $exe @()
    if ((Get-LinkCalibrationIdentity $exe -Amd64Pe).sha256 -cne $state.executable.sha256) { throw 'control_executable_changed' }
    $state.status = 'observed'
} catch {
    $state['failure_phase'] = $phase
} finally {
    Pop-Location
    $state | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $report -Encoding utf8
}
if ($state.status -cne 'observed') { exit 1 }
