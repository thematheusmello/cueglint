# Non-distributing qualification of a frozen public source on a fresh Windows x64 host.
# No package helper, upload, signing or model/capture action is included.
param([Parameter(Mandatory)][string]$Repo,
      [Parameter(Mandatory)][string]$Out,
      [Parameter(Mandatory)][string]$Target,
      [ValidateSet('frozen', 'package-candidate')][string]$SourceSet = 'frozen',
      [switch]$GuiFixture,
      [switch]$ProvenanceOnly,
      [switch]$LinkCalibrationOnly)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ARCH -ne 'X64' -or
    $env:GITHUB_EVENT_NAME -ne 'workflow_dispatch' -or $env:GITHUB_REF -ne 'refs/heads/main') {
    throw 'Only a fresh GitHub-hosted Windows x64 qualification job is in scope.'
}
$Repo = [IO.Path]::GetFullPath($Repo)
$Out = [IO.Path]::GetFullPath($Out)
$Target = [IO.Path]::GetFullPath($Target)
foreach ($path in @($Repo, $Out, $Target)) {
    if ($path -match '[%&|<>^"\r\n]') { throw 'Unsupported command-file path characters.' }
}
foreach ($path in @($Out, $Target)) {
    if (-not $path.StartsWith([IO.Path]::GetFullPath($env:RUNNER_TEMP) + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Evidence/build outputs must be inside RUNNER_TEMP.' }
    if (Test-Path -LiteralPath $path) { throw 'Use fresh output paths; preserve an earlier attempt.' }
}
New-Item -ItemType Directory -Path $Out, $Target | Out-Null
Set-Location -LiteralPath $Repo
$state = [ordered]@{
    source_set = $SourceSet
    source = $null
    controller_commit = $env:GITHUB_SHA
    event = $env:GITHUB_EVENT_NAME
    run_id = $env:GITHUB_RUN_ID
    run_attempt = $env:GITHUB_RUN_ATTEMPT
    provenance_only = [bool]$ProvenanceOnly
    link_calibration_only = [bool]$LinkCalibrationOnly
    distribution_ready = $false
    windows_notice_ready = $false
    windows_notice_adoption = $false
    stages = [ordered]@{}
    license_findings = @()
    completed = $false
}
if ($GuiFixture) { $state['gui_fixture'] = [ordered]@{ fixture = 'native-v2-dashboard'; passed = $false; started = $false } }
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Native([string]$Name, [string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments 1> (Join-Path $Out "$Name.stdout.txt") 2> (Join-Path $Out "$Name.stderr.txt")
    $code = $LASTEXITCODE
    $state.stages[$Name] = $code
    foreach ($stream in @('stdout','stderr')) {
        if ((Get-Item -LiteralPath (Join-Path $Out "$Name.$stream.txt")).Length -gt 32MB) { throw "$Name exceeded the post-command log limit." }
    }
    if ($code -ne 0) { throw "$Name failed with exit $code; retain its original logs." }
}
function Verify-Inputs($Expected) {
    foreach ($item in $Expected.inputs) {
        if ((Get-Item -LiteralPath (Join-Path $Repo $item.source_path)).Length -ne $item.bytes -or
            (Hash (Join-Path $Repo $item.source_path)) -ne $item.sha256) { throw "Input drift: $($item.source_path)" }
    }
    if ((Hash (Join-Path $Repo 'third-party/runtime/manifest.json')) -ne $Expected.notice_manifest_sha256) { throw 'macOS source inventory drift.' }
    if ($Expected.windows_notice_manifest_sha256 -and
        (Hash (Join-Path $Repo 'third-party/runtime/windows-x86_64-msvc/manifest.json')) -ne $Expected.windows_notice_manifest_sha256) {
        throw 'Windows source inventory drift.'
    }
}
function Complete-HostChecks($Expected) {
    Verify-Inputs $Expected
    Native 'source-clean-after' 'git' @('status','--porcelain')
    if ((Get-Item (Join-Path $Out 'source-clean-after.stdout.txt')).Length) { throw 'Source changed during qualification.' }
    if (-not ($ProvenanceOnly -or $LinkCalibrationOnly)) { $state['technical_checks_completed'] = $true }
    $state['bounded_notice_discovery_satisfied'] = ($state.license_findings.Count -eq 0)
    if (-not $state.bounded_notice_discovery_satisfied) { throw 'Required host notice/discovery evidence is incomplete; provisional technical results are retained.' }
}
function Pe-Identity([string]$Path) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Missing MZ signature.' }
        $reader.BaseStream.Position = 0x3c; $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or $offset + 26 -gt $reader.BaseStream.Length) { throw 'Invalid PE offset.' }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Expected PE AMD64.' }
        $reader.BaseStream.Position = $offset + 24
        if ($reader.ReadUInt16() -ne 0x20b) { throw 'Expected PE32+.' }
    } finally { $reader.Dispose() }
}
function Get-CargoOutputDirectory([string]$TargetRoot, $OutDir) {
    $invalid = @{ reason = 'cargo_out_dir_not_canonical_under_target' }
    if ($OutDir -isnot [string] -or $OutDir.Length -gt 1024 -or -not [IO.Path]::IsPathFullyQualified($OutDir)) { return $invalid }
    $root = [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetFullPath($TargetRoot))
    $prefix = $root + [IO.Path]::DirectorySeparatorChar
    if ([IO.Path]::GetFullPath($OutDir) -cne $OutDir -or -not $OutDir.StartsWith($prefix, [StringComparison]::Ordinal)) { return $invalid }
    $relative = $OutDir.Substring($prefix.Length)
    $parts = $relative.Split([IO.Path]::DirectorySeparatorChar)
    if ($parts.Count -gt 16 -or @($parts | Where-Object { $_ -cnotmatch '^[A-Za-z0-9_.-]+$' -or $_.EndsWith('.') }).Count) { return $invalid }
    $invalid.reason = 'cargo_out_dir_missing_or_reparse'
    $directory = $root
    foreach ($part in @('') + $parts) {
        if ($part) { $directory = Join-Path $directory $part }
        $item = Get-Item -LiteralPath $directory -ErrorAction SilentlyContinue
        if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { return $invalid }
    }
    return @{ path = $OutDir; relative = $relative.Replace([IO.Path]::DirectorySeparatorChar, [char]'/') }
}
function Read-WglGeneratedBindings([string]$MetadataPath, [string]$BuildLog, [string]$TargetRoot, [string]$MetadataHash) {
    $receipt = [ordered]@{
        package = 'glutin_wgl_sys@0.6.1'; status = 'refused'; reason = 'cargo_input_unavailable'
        files = @(); file_limit_bytes = 128KB; cargo_input_limit_bytes = 32MB
    }
    try {
        foreach ($path in @($MetadataPath, $BuildLog)) {
            $item = Get-Item -LiteralPath $path
            if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
                $item.Length -le 0 -or $item.Length -gt $receipt.cargo_input_limit_bytes) { return $receipt }
        }
        $receipt.reason = 'cargo_metadata_identity_changed'
        $receipt['metadata_sha256'] = Hash $MetadataPath
        if ($receipt.metadata_sha256 -cne $MetadataHash) { return $receipt }
        $receipt.reason = 'cargo_package_identity_missing_or_ambiguous'
        $metadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
        $package = @($metadata.packages | Where-Object {
            $_.name -ceq 'glutin_wgl_sys' -and $_.version -ceq '0.6.1' -and
            $_.source -ceq 'registry+https://github.com/rust-lang/crates.io-index'
        })
        if ($package.Count -ne 1 -or $package[0].id -isnot [string] -or -not $package[0].id -or
            $package[0].id.Length -gt 512) { return $receipt }
        $receipt['cargo_package_id'] = $package[0].id
        $receipt.reason = 'cargo_build_script_message_missing_or_ambiguous'
        $receipt['build_log_sha256'] = Hash $BuildLog
        $build = $null
        $reader = [IO.File]::OpenText($BuildLog)
        try {
            while ($null -ne ($line = $reader.ReadLine())) {
                if (-not $line.Contains('"build-script-executed"', [StringComparison]::Ordinal)) { continue }
                $message = ConvertFrom-Json -InputObject $line
                if ($message.reason -cne 'build-script-executed' -or $message.package_id -cne $receipt.cargo_package_id) { continue }
                if ($null -ne $build) { return $receipt }
                $build = $message
            }
        } finally { $reader.Dispose() }
        if ($null -eq $build) { return $receipt }
        $location = Get-CargoOutputDirectory $TargetRoot $build.out_dir
        if (-not $location.path) { $receipt.reason = $location.reason; return $receipt }
        $outDir = $location.path
        $receipt['target_relative_out_dir'] = $location.relative
        $receipt.status = 'partial'
        $receipt.reason = 'generated_binding_identity_incomplete'
        $receipt.files = @(foreach ($name in @('wgl_bindings.rs', 'wgl_extra_bindings.rs')) {
            $identity = [ordered]@{ file = $name; status = 'unavailable'; reason = 'generated_file_missing' }
            try {
                $path = Join-Path $outDir $name
                if (Test-Path -LiteralPath $path) {
                    $identity.status = 'refused'; $identity.reason = 'generated_file_type_or_size'
                    $item = Get-Item -LiteralPath $path
                    if (-not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                        $identity['bytes'] = $item.Length
                    }
                    if ($identity.bytes -gt 0 -and $identity.bytes -le $receipt.file_limit_bytes) {
                        $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
                        try {
                            if ($stream.Length -ne $item.Length) { throw 'Generated file length changed.' }
                            $bytes = [byte[]]::new([int]$stream.Length)
                            $stream.ReadExactly($bytes, 0, $bytes.Length)
                            if ($stream.ReadByte() -ne -1) { throw 'Generated file grew.' }
                        } finally { $stream.Dispose() }
                        $identity['sha256'] = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
                        $identity.status = 'partial'; $identity.reason = 'reviewed_type_source_markers_incomplete'
                        $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes)
                        $markers = [ordered]@{}
                        foreach ($header in @('WinNT.h','Windef.h','BaseTsd.h','IntSafe.h','Wingdi.h')) {
                            $markers[$header] = $text -cmatch ('(?m)^[ \t]*// From ' + [regex]::Escape($header) + '\r?$')
                        }
                        $identity['type_source_markers'] = $markers
                        if ($markers.Values -notcontains $false) { $identity.status = 'observed'; $identity.reason = 'digest_and_reviewed_markers' }
                    }
                }
            } catch { $identity['error_type'] = $_.Exception.GetBaseException().GetType().Name }
            $identity
        })
        if (@($receipt.files | Where-Object { $_.status -ceq 'observed' }).Count -eq 2) {
            $receipt.status = 'observed'; $receipt.reason = 'both_generated_bindings_identified'
        }
    } catch { $receipt['error_type'] = $_.Exception.GetBaseException().GetType().Name }
    return $receipt
}
function Read-BoundedBinaryIdentity([string]$Path) {
    $item = Get-Item -LiteralPath $Path
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $item.Length -le 0 -or $item.Length -gt 32MB) { return $null }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -ne $item.Length) { return $null }
        $digest = [Security.Cryptography.SHA256]::HashData($stream)
        if ($stream.Position -ne $item.Length -or $stream.Length -ne $item.Length) { return $null }
        return @{ bytes = $item.Length; sha256 = [Convert]::ToHexString($digest).ToLowerInvariant() }
    } finally { $stream.Dispose() }
}
function Read-SelectedLinkerIdentity([string]$Paths, [string]$Expected) {
    $receipt = [ordered]@{ status = 'refused'; reason = 'linker_resolution_unavailable'; file = 'link.exe' }
    try {
        $item = Get-Item -LiteralPath $Paths
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $item.Length -le 0 -or $item.Length -gt 8KB) { return $receipt }
        $receipt.reason = 'first_linker_differs_from_selected_toolset'
        if ((Get-Content -LiteralPath $Paths -TotalCount 1).Trim() -ine $Expected) { return $receipt }
        $receipt.reason = 'linker_file_identity_incomplete'
        $identity = Read-BoundedBinaryIdentity $Expected
        if ($null -eq $identity) { return $receipt }
        $receipt['bytes'] = $identity.bytes
        $receipt['sha256'] = $identity.sha256
        $receipt.status = 'partial'; $receipt.reason = 'linker_file_version_unavailable'
        $version = [Diagnostics.FileVersionInfo]::GetVersionInfo($Expected)
        if ($version.FileVersion -and $version.FileMajorPart -gt 0) {
            $receipt['file_version'] = '{0}.{1}.{2}.{3}' -f $version.FileMajorPart, $version.FileMinorPart,
                $version.FileBuildPart, $version.FilePrivatePart
            $receipt.status = 'observed'; $receipt.reason = 'selected_path_and_disk_identity'
        }
    } catch { $receipt['error_type'] = $_.Exception.GetBaseException().GetType().Name }
    return $receipt
}
function Read-DeclaredNativeArchives([string]$MetadataPath, [string]$MetadataHash, [object[]]$BuildScripts, [string]$TargetRoot) {
    $receipt = [ordered]@{ status = 'refused'; reason = 'cargo_metadata_unavailable'; packages = @(); archive_limit_bytes = 32MB }
    try {
        $item = Get-Item -LiteralPath $MetadataPath
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $item.Length -le 0 -or $item.Length -gt 32MB) { return $receipt }
        $receipt.reason = 'cargo_metadata_identity_changed'
        if ((Hash $MetadataPath) -cne $MetadataHash) { return $receipt }
        $metadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
        $receipt.status = 'partial'; $receipt.reason = 'declared_archive_identity_incomplete'
        foreach ($plan in @(
            @{ name = 'ring'; version = '0.17.14'; libraries = @('ring_core_0_17_14_', 'ring_core_0_17_14__test') },
            @{ name = 'libsqlite3-sys'; version = '0.36.0'; libraries = @('sqlite3') }
        )) {
            $row = [ordered]@{
                package = "$($plan.name)@$($plan.version)"; status = 'refused'
                reason = 'cargo_package_missing_or_ambiguous'; declared_libraries = @(); archives = @()
            }
            $receipt.packages += $row
            try {
                $package = @($metadata.packages | Where-Object {
                    $_.name -ceq $plan.name -and $_.version -ceq $plan.version -and
                    $_.source -ceq 'registry+https://github.com/rust-lang/crates.io-index'
                })
                if ($package.Count -ne 1 -or $package[0].id -isnot [string] -or
                    -not $package[0].id -or $package[0].id.Length -gt 512) { continue }
                $row.reason = 'cargo_build_script_missing_or_ambiguous'
                $script = @($BuildScripts | Where-Object { $_.package_id -ceq $package[0].id })
                if ($script.Count -ne 1) { continue }
                $row.reason = 'invalid_linked_lib_names'
                $names = @($script[0].linked_libs)
                if ($names.Count -gt 8 -or @($names | Where-Object {
                    $_ -isnot [string] -or $_ -cnotmatch '^[A-Za-z0-9_+=.:-]{1,128}$'
                }).Count) { continue }
                $row.declared_libraries = $names
                $row['cargo_package_id'] = $package[0].id
                $location = Get-CargoOutputDirectory $TargetRoot $script[0].out_dir
                if (-not $location.path) { $row.reason = $location.reason; continue }
                $outDir = $location.path
                $row['target_relative_out_dir'] = $location.relative
                $row.status = 'partial'; $row.reason = 'native_out_dir_not_declared_for_search'
                if ($script[0].linked_paths -cnotcontains "native=$outDir") { continue }
                $row.reason = 'static_library_or_archive_incomplete'
                foreach ($name in $plan.libraries) {
                    $archive = [ordered]@{ file = "$name.lib"; status = 'unavailable' }
                    $row.archives += $archive
                    if ($names -cnotcontains "static=$name") { continue }
                    $file = Join-Path $outDir $archive.file
                    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { continue }
                    $identity = Read-BoundedBinaryIdentity $file
                    if ($null -eq $identity) { $archive.status = 'refused'; continue }
                    $archive['bytes'] = $identity.bytes
                    $archive['sha256'] = $identity.sha256
                    $archive.status = 'observed'
                }
                if (@($row.archives | Where-Object { $_.status -ceq 'observed' }).Count -eq $plan.libraries.Count) {
                    $row.status = 'observed'; $row.reason = 'declared_static_archive_identities'
                }
            } catch { $row['error_type'] = $_.Exception.GetBaseException().GetType().Name }
        }
        if (@($receipt.packages | Where-Object { $_.status -ceq 'observed' }).Count -eq 2) {
            $receipt.status = 'observed'; $receipt.reason = 'both_declared_native_archive_sets'
        }
    } catch { $receipt['error_type'] = $_.Exception.GetBaseException().GetType().Name }
    return $receipt
}
try {
    if ($ProvenanceOnly -and $GuiFixture) { throw 'provenance_only cannot be combined with gui_fixture.' }
    if ($LinkCalibrationOnly -and ($GuiFixture -or $ProvenanceOnly)) {
        throw 'Select link_calibration_only by itself.'
    }
    $inputs = @{
        'frozen' = @{
            name = 'expected-source-inputs.json'
            sha256 = 'e3ec2c314e2fbbf4c4334cc49bbf9f0b307bf6e5abd674b335fed265dabe2a4c'
        }
        'package-candidate' = @{
            name = 'expected-package-candidate-inputs.json'
            sha256 = 'd402a44d1243c78f28b90b8fb0eb6f6484d1597ef459f05d100baf31a1df07a8'
        }
    }[$SourceSet]
    $inputFile = Join-Path $PSScriptRoot $inputs.name
    if ((Hash $inputFile) -cne $inputs.sha256) { throw 'Selected source input data changed.' }
    $expected = Get-Content -LiteralPath $inputFile -Raw | ConvertFrom-Json
    $state.source = $expected.source_commit
    if ($expected.windows_notice_manifest_sha256) {
        $state['windows_source_inventory_sha256'] = $expected.windows_notice_manifest_sha256
    }
    $graphFile = Join-Path $PSScriptRoot 'expected-graph.json'
    if ((Hash $graphFile) -ne '92ad62b3c575274831b7d409babdac4df71cc4c872cf14aa7bcbe178d3ba3988') { throw 'Frozen graph data changed.' }
    Native 'controller-head' 'git' @('-C',$PSScriptRoot,'rev-parse','HEAD')
    if ((Get-Content (Join-Path $Out 'controller-head.stdout.txt') -Raw).Trim() -cne $env:GITHUB_SHA) { throw 'Wrong workflow controller commit.' }
    Native 'controller-clean' 'git' @('-C',$PSScriptRoot,'status','--porcelain')
    if ((Get-Item (Join-Path $Out 'controller-clean.stdout.txt')).Length) { throw 'Controller checkout is not clean.' }
    $state['controller_files'] = @('qualify.ps1','summarize.ps1','gui-fixture.ps1','link-calibration.ps1',$inputs.name,'expected-graph.json') | ForEach-Object {
        $file = Join-Path $PSScriptRoot $_
        @{ file = $_; bytes = (Get-Item -LiteralPath $file).Length; sha256 = Hash $file }
    }
    $state['graph_rebind'] = $expected.graph_rebind
    Native 'source-head' 'git' @('rev-parse','HEAD')
    if ((Get-Content (Join-Path $Out 'source-head.stdout.txt') -Raw).Trim() -ne $expected.source_commit) { throw 'Wrong public source commit.' }
    Native 'source-tree' 'git' @('rev-parse','HEAD^{tree}')
    if ((Get-Content (Join-Path $Out 'source-tree.stdout.txt') -Raw).Trim() -ne $expected.source_tree) { throw 'Wrong public source tree.' }
    Native 'source-clean' 'git' @('status','--porcelain')
    if ((Get-Item (Join-Path $Out 'source-clean.stdout.txt')).Length) { throw 'Source checkout is not clean.' }
    Verify-Inputs $expected
    foreach ($name in @('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET','CARGO_BUILD_RUSTFLAGS')) {
        if ([Environment]::GetEnvironmentVariable($name)) { throw "Unexpected build override: $name" }
    }
    $env:CARGO_TARGET_DIR = $Target; $env:CARGO_BUILD_JOBS = '2'; $env:CARGO_TERM_COLOR = 'never'
    Native 'rustc-version' 'rustc' @('+1.94.1','-vV')
    if ((Get-Content (Join-Path $Out 'rustc-version.stdout.txt') -Raw) -notmatch 'host: x86_64-pc-windows-msvc') { throw 'Compiler is not native Windows x64.' }
    Native 'cargo-version' 'cargo' @('+1.94.1','-V')
    Native 'sysroot' 'rustc' @('+1.94.1','--print','sysroot')
    $sysroot = (Get-Content (Join-Path $Out 'sysroot.stdout.txt') -Raw).Trim()
    $notice = Join-Path $sysroot 'share/doc/rust/COPYRIGHT-library.html'
    $rustNotice = if (Test-Path -LiteralPath $notice -PathType Leaf) { @{ bytes = (Get-Item $notice).Length; sha256 = Hash $notice } } else { $null }
    $state['rust_notice'] = $rustNotice
    $state['rust_installer_records'] = @(
        foreach ($relativePath in @('lib\rustlib\manifest-rustc-x86_64-pc-windows-msvc','lib\rustlib\manifest-rust-std-x86_64-pc-windows-msvc','lib\rustlib\multirust-channel-manifest.toml')) {
            $file = Join-Path $sysroot $relativePath
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
                $state.license_findings += "Missing installed Rust record: $relativePath."
                continue
            }
            $item = Get-Item -LiteralPath $file
            if ($item.Length -le 0 -or $item.Length -gt 2MB -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                $state.license_findings += "Invalid installed Rust record: $relativePath."
                continue
            }
            @{ relative_path = $relativePath; bytes = $item.Length; sha256 = Hash $file }
        }
    )
    $state['rust_tool_binaries'] = @('bin\rustc.exe','bin\cargo.exe') | ForEach-Object { @{ relative_path = $_; sha256 = Hash (Join-Path $sysroot $_) } }
    if ($state.license_findings.Count) {
        $state['bounded_notice_discovery_satisfied'] = $false
        throw 'Required installed Rust component records are incomplete.'
    }
    if (-not $rustNotice -or $rustNotice.sha256 -ne 'af70aaabed1b73e872f14f9130db37e09f3f4d73a5f7c598b9173697a5d2729f') { $state.license_findings += 'Windows-host Rust notice is missing/different; do not substitute macOS approval.' }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    Native 'vswhere' $vswhere @('-latest','-products','*','-requires','Microsoft.VisualStudio.Component.VC.Tools.x86.x64','-format','json','-utf8')
    $vs = @(Get-Content (Join-Path $Out 'vswhere.stdout.txt') -Raw | ConvertFrom-Json)[0]
    if (-not $vs.installationPath) { throw 'No installed native C++ toolchain.' }
    $vcVersion = (Get-Content (Join-Path $vs.installationPath 'VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt') -Raw).Trim()
    if ($vcVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Unexpected MSVC toolset identity.' }
    $vcRoot = Join-Path $vs.installationPath "VC\Tools\MSVC\$vcVersion"
    $sdkRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10).KitsRoot10
    $sdkVersion = '10.0.26100.0'
    if (-not (Test-Path (Join-Path $sdkRoot "Lib\$sdkVersion\um\x64"))) { throw 'Documented SDK is not installed; retain this source gap.' }
    $state['tools'] = @{ image_os = $env:ImageOS; image_version = $env:ImageVersion; vs_version = $vs.installationVersion; msvc_version = $vcVersion; sdk_version = $sdkVersion; cl_sha256 = Hash (Join-Path $vcRoot 'bin\Hostx64\x64\cl.exe'); dumpbin_sha256 = Hash (Join-Path $vcRoot 'bin\Hostx64\x64\dumpbin.exe') }
    $licenseFiles = @()
    $licenseRoots = @(@{ path = $sdkRoot; depth = 0 }, @{ path = $vs.installationPath; depth = 0 },
        @{ path = (Join-Path $sdkRoot 'Licenses'); depth = 2 },
        @{ path = (Join-Path $vs.installationPath 'Licenses'); depth = 2 },
        @{ path = (Join-Path $vs.installationPath 'VC\Redist\MSVC'); depth = 2 })
    $licenseBytes = 0
    foreach ($entry in $licenseRoots) {
        $directory = $entry.path
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) { continue }
        $children = if ($entry.depth -eq 0) { Get-ChildItem -LiteralPath $directory -File } else { Get-ChildItem -LiteralPath $directory -File -Recurse -Depth 2 }
        $found = @($children | Where-Object { $_.Name -match '(?i)(licen[cs]e|eula|copyright|redist)' -and $_.Extension -in @('.txt','.rtf','.html','.md') -and -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) })
        if ($found.Count -gt 64) { throw 'License discovery bound reached; keep the finding, do not silently truncate.' }
        foreach ($file in $found) {
            if ($file.Length -gt 2MB) { throw 'License file bound reached.' }
            $licenseBytes += $file.Length
            if ($licenseFiles.Count -ge 64 -or $licenseBytes -gt 16MB) { throw 'Aggregate legal-file discovery bound reached.' }
            $licenseFiles += @{ tool_root = $directory; relative_path = [IO.Path]::GetRelativePath($directory, $file.FullName); bytes = $file.Length; sha256 = Hash $file.FullName }
            $state['license_files'] = $licenseFiles
        }
    }
    $state['license_files'] = $licenseFiles
    if (-not $licenseFiles.Count) { $state.license_findings += 'Installed SDK/Visual Studio terms not located by bounded discovery; original terms/rights remain unresolved.' }
    Native 'metadata' 'cargo' @('+1.94.1','metadata','--locked','--filter-platform','x86_64-pc-windows-msvc','--format-version','1')
    Native 'tree' 'cargo' @('+1.94.1','tree','--locked','--target','x86_64-pc-windows-msvc','-p','cueglint','-p','cueglint-cli','--edges','normal,build','--prefix','none','--format','{p}|{l}|{f}')
    $actual = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    foreach ($line in Get-Content (Join-Path $Out 'tree.stdout.txt')) {
        if (-not $line.Trim()) { continue }
        $parts = $line.Split('|')
        if ($parts.Count -ne 3 -or $parts[0] -notmatch '^([A-Za-z0-9_-]+) v([0-9A-Za-z.+-]+)') { throw 'Unexpected tree output.' }
        $id = $Matches[1] + '@' + $Matches[2]
        if (-not $actual.ContainsKey($id)) { $actual[$id] = @{ license = $parts[1]; features = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal) } }
        if ($actual[$id].license -cne $parts[1]) { throw 'Conflicting package license rows.' }
        foreach ($feature in ($parts[2] -replace ' \(\*\)$','').Split(',')) { if ($feature) { [void]$actual[$id].features.Add($feature) } }
    }
    $graph = Get-Content $graphFile -Raw | ConvertFrom-Json
    $differences = @()
    foreach ($package in $graph.packages) {
        if (-not $actual.ContainsKey($package.id)) { $differences += "missing:$($package.id)"; continue }
        if ($actual[$package.id].license -cne $package.license_expression_reported -or -not $actual[$package.id].features.SetEquals([string[]]$package.features)) { $differences += "changed:$($package.id)" }
    }
    $expectedIds = [Collections.Generic.HashSet[string]]::new([string[]]$graph.packages.id, [StringComparer]::Ordinal)
    foreach ($id in $actual.Keys) { if (-not $expectedIds.Contains($id)) { $differences += "extra:$id" } }
    $state['native_graph'] = @{ packages = $actual.Count; expected = $graph.packages.Count; differences = $differences.Count; expected_graph_sha256 = Hash $graphFile; tree_sha256 = Hash (Join-Path $Out 'tree.stdout.txt'); metadata_sha256 = Hash (Join-Path $Out 'metadata.stdout.txt') }
    ConvertTo-Json -InputObject @($differences) | Set-Content (Join-Path $Out 'graph-diff.json') -Encoding utf8
    if ($differences.Count -or $actual.Count -ne 397) { throw 'Native-host graph drift; no automatic adoption/retry.' }
    $vcvars = Join-Path $vs.installationPath 'VC\Auxiliary\Build\vcvarsall.bat'
    $stage = 'vcvars-only'
    $cmd = Join-Path $Out 'vcvars-only.cmd'
    $command = 'exit /b %errorlevel%'
    if ($LinkCalibrationOnly) {
        $stage = 'link-calibration-launch'
        $cmd = Join-Path $Out 'link-calibration.cmd'
        $launchLogs = Join-Path $Out 'link-calibration-launch'
        $null = New-Item -ItemType Directory -Path $launchLogs
        $pwsh = Join-Path $PSHOME 'pwsh.exe'
        $helper = Join-Path $PSScriptRoot 'link-calibration.ps1'
        $command = @"
"$pwsh" -NoProfile -NonInteractive -File "$helper" -Out "$Out" -VcRoot "$vcRoot" > "$launchLogs\stdout.txt" 2> "$launchLogs\stderr.txt"
exit /b %errorlevel%
"@
    } elseif (-not $ProvenanceOnly) {
        $stage = 'build-launch'
        $cmd = Join-Path $Out 'build.cmd'
        $command = @"
cargo +1.94.1 build --release --locked --message-format=json --target x86_64-pc-windows-msvc -p cueglint -p cueglint-cli > "$Out\build.jsonl" 2> "$Out\build.stderr.txt"
exit /b %errorlevel%
"@
        if ($SourceSet -ceq 'package-candidate') {
            $command = "where.exe link.exe > ""$Out\selected-link-paths.txt"" 2>nul" + [Environment]::NewLine + $command
        }
    }
    @"
@echo off
call "$vcvars" x64 $sdkVersion -vcvars_ver=$vcVersion > "$Out\vcvars.stdout.txt" 2> "$Out\vcvars.stderr.txt"
if errorlevel 1 exit /b %errorlevel%
(echo WindowsSDKVersion=%WindowsSDKVersion%&echo VCToolsVersion=%VCToolsVersion%&echo VCToolsInstallDir=%VCToolsInstallDir%) > "$Out\selected-tool-vars.txt"
$command
"@ | Set-Content -LiteralPath $cmd -Encoding ascii
    Native $stage $env:ComSpec @('/d','/c',$cmd)
    $selected = @{}
    foreach ($line in Get-Content (Join-Path $Out 'selected-tool-vars.txt')) {
        if ($line -notmatch '^(WindowsSDKVersion|VCToolsVersion|VCToolsInstallDir)=(.+)$' -or $selected.ContainsKey($Matches[1])) { throw 'Unexpected selected tool identity.' }
        $selected[$Matches[1]] = $Matches[2]
    }
    $state['selected_tools'] = $selected
    if ($selected.Count -ne 3 -or $selected.WindowsSDKVersion.TrimEnd('\') -cne $sdkVersion -or
        $selected.VCToolsVersion -cne $vcVersion -or
        [IO.Path]::GetFullPath($selected.VCToolsInstallDir).TrimEnd('\') -ine $vcRoot.TrimEnd('\')) {
        throw 'Actual vcvars selection differs from requested SDK/MSVC; retain the build evidence without qualification.'
    }
    if ($LinkCalibrationOnly) {
        $receipt = Join-Path $Out 'link-calibration.json'
        $item = Get-Item -LiteralPath $receipt
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $item.Length -le 0 -or $item.Length -gt 16KB) { throw 'Invalid calibration receipt.' }
        $calibration = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
        if ($calibration.schema_version -ne 1 -or $calibration.status -cne 'observed' -or
            $calibration.msvc -cne $vcVersion -or $calibration.sdk -cne $sdkVersion) {
            throw 'The calibration did not verify the selected tools.'
        }
        $state['link_calibration_sha256'] = Hash $receipt
    }
    if ($ProvenanceOnly -or $LinkCalibrationOnly) {
        Complete-HostChecks $expected
        $state.completed = $true
        return
    }
    foreach ($name in @('build.jsonl','build.stderr.txt')) { if ((Get-Item (Join-Path $Out $name)).Length -gt 32MB) { throw 'Build log limit reached.' } }
    if ($SourceSet -ceq 'package-candidate') {
        $state['wgl_generated_bindings'] = Read-WglGeneratedBindings (Join-Path $Out 'metadata.stdout.txt') (Join-Path $Out 'build.jsonl') $Target $state.native_graph.metadata_sha256
    }
    $messages = @(Get-Content -LiteralPath (Join-Path $Out 'build.jsonl') | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
        ($_.reason -ceq 'build-script-executed' -and $SourceSet -ceq 'package-candidate') -or
        ($_.reason -ceq 'compiler-artifact' -and $_.executable -and $_.target.name -in @('cueglint','cueglint-cli'))
    })
    if ($SourceSet -ceq 'package-candidate') {
        $state['selected_linker'] = Read-SelectedLinkerIdentity (Join-Path $Out 'selected-link-paths.txt') (Join-Path $vcRoot 'bin\Hostx64\x64\link.exe')
        $scripts = @($messages | Where-Object { $_.reason -ceq 'build-script-executed' })
        $state['declared_native_archives'] = Read-DeclaredNativeArchives (Join-Path $Out 'metadata.stdout.txt') $state.native_graph.metadata_sha256 $scripts $Target
    }
    $artifacts = @($messages | Where-Object { $_.reason -ceq 'compiler-artifact' })
    if ($artifacts.Count -ne 2 -or @($artifacts.target.name | Sort-Object -Unique).Count -ne 2) { throw 'Expected exactly two Cargo-reported executables.' }
    $exe = @{}
    foreach ($artifact in $artifacts) {
        $path = [IO.Path]::GetFullPath($artifact.executable)
        if (-not $path.StartsWith($Target + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Executable is outside the dedicated target.' }
        Pe-Identity $path; $exe[$artifact.target.name] = $path
    }
    $state['executables'] = @($exe.GetEnumerator() | ForEach-Object { @{ name = $_.Key; sha256 = Hash $_.Value; bytes = (Get-Item $_.Value).Length; PE = 'AMD64 PE32+' } })
    $inspect = Join-Path $Out 'inspect.cmd'; $dumpbin = Join-Path $vcRoot 'bin\Hostx64\x64\dumpbin.exe'
    @"
@echo off
call "$vcvars" x64 $sdkVersion -vcvars_ver=$vcVersion >nul 2> "$Out\inspect-vcvars.stderr.txt"
if errorlevel 1 exit /b %errorlevel%
"$dumpbin" /headers /dependents "$($exe['cueglint'])" > "$Out\gui-pe.txt" 2>&1
if errorlevel 1 exit /b %errorlevel%
"$dumpbin" /headers /dependents "$($exe['cueglint-cli'])" > "$Out\cli-pe.txt" 2>&1
exit /b %errorlevel%
"@ | Set-Content -LiteralPath $inspect -Encoding ascii
    Native 'inspect-launch' $env:ComSpec @('/d','/c',$inspect)
    $state['imports'] = @('gui-pe.txt','cli-pe.txt') | ForEach-Object { @{ report = $_; dll_names = @(Get-Content (Join-Path $Out $_) | Where-Object { $_ -match '^\s+[A-Za-z0-9_.-]+\.dll\s*$' } | ForEach-Object { $_.Trim() } | Sort-Object -Unique) } }
    $isolated = Join-Path $Out 'isolated-data'; $models = Join-Path $Out 'empty-models'
    New-Item -ItemType Directory -Path $isolated, $models | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new($exe['cueglint-cli'])
    $start.WorkingDirectory = $isolated; $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $start.Environment['CUEGLINT_DATA_DIR'] = $isolated
    foreach ($argument in @('--db',(Join-Path $isolated 'state.sqlite3'),'--models-dir',$models,'--help')) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($start); $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
    $state['cli_help'] = @{ pid = $process.Id; timeout_ms = 10000; timed_out = $false }
    try {
        if (-not $process.WaitForExit(10000)) {
            $state.cli_help.timed_out = $true
            $process.Kill($true)
            if (-not $process.WaitForExit(5000)) { throw 'CLI help cleanup was not confirmed after timeout.' }
        }
        if (-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($stdout,$stderr), 2000)) { throw 'CLI help streams did not close.' }
        [IO.File]::WriteAllText((Join-Path $Out 'cli-help.stdout.txt'),$stdout.GetAwaiter().GetResult())
        [IO.File]::WriteAllText((Join-Path $Out 'cli-help.stderr.txt'),$stderr.GetAwaiter().GetResult())
        $state['cli_help_exit_code'] = $process.ExitCode
    } finally { $process.Dispose() }
    if ($state.cli_help.timed_out) { throw 'CLI help timeout; original streams retained, no retry.' }
    if ($state.cli_help_exit_code -ne 0) { throw 'CLI help failed.' }
    $help = $stdout.GetAwaiter().GetResult()
    if ($help -notmatch '(?m)^Usage: cueglint-cli\.exe \[OPTIONS\] <COMMAND>\r?$' -or $help -notmatch '(?m)^Commands:\r?$') {
        throw 'CLI help output is missing its usage and command list.'
    }
    if (@(Get-ChildItem -LiteralPath $isolated -Force).Count -or @(Get-ChildItem -LiteralPath $models -Force).Count) { throw 'Help unexpectedly wrote isolated data/model files.' }
    Complete-HostChecks $expected
    if ($GuiFixture) {
        . (Join-Path $PSScriptRoot 'gui-fixture.ps1')
        Invoke-GuiFixture -Executable $exe['cueglint'] -Out $Out -State $state.gui_fixture
        Verify-Inputs $expected
        Native 'gui-source-clean-after' 'git' @('status','--porcelain')
        if ((Get-Item (Join-Path $Out 'gui-source-clean-after.stdout.txt')).Length) { throw 'Source changed during GUI qualification.' }
    }
    $state.completed = $true
} catch {
    $state['failure'] = $_.Exception.Message
    throw
} finally {
    $state | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $Out 'qualification.json') -Encoding utf8
    # Evidence stays separate from Target. No upload/copy/package/sign step is present.
}
