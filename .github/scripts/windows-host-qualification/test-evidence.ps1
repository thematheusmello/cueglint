$ErrorActionPreference = 'Stop'
$syntax = @{}
foreach ($name in @('qualify.ps1', 'gui-fixture.ps1')) {
    $tokens = $null
    $errors = $null
    $syntax[$name] = [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
foreach ($name in @('Hash', 'Get-CargoOutputDirectory', 'Read-WglGeneratedBindings', 'Read-BoundedBinaryIdentity',
    'Read-SelectedLinkerIdentity', 'Read-DeclaredNativeArchives')) {
    $definition = $syntax['qualify.ps1'].Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name
    }, $true)
    . ([ScriptBlock]::Create($definition.Extent.Text))
}
$probe = $syntax['gui-fixture.ps1'].Find({
    param($node)
    $node -is [System.Management.Automation.Language.StringConstantExpressionAst] -and
        $node.Value.Contains('public static class CueglintGuiDllProbe')
}, $true)
Add-Type -TypeDefinition $probe.Value
$paths = [Collections.Generic.SortedSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$expected = @('C:/Windows/System32/VCRUNTIME140.dll', 'C:/Windows/System32/ucrtbase.dll',
    'C:/Windows/System32/msvcrt.dll', 'C:/Outside/vcruntime140.dll', 'C:/Windows/System32/d3d12.dll')
foreach ($path in $expected) { [void]$paths.Add($path) }
$order = [CueglintGuiDllProbe].GetMethod('IdentityOrder', [Reflection.BindingFlags]'Static, NonPublic')
$actual = @($order.Invoke($null, @($paths, 'C:/Windows/', 'C:/Windows/System32/')))
if (($actual -join '|') -cne ($expected -join '|')) { throw 'Unexpected CRT priority.' }

$temp = [IO.Directory]::CreateTempSubdirectory('cueglint-evidence-check-')
try {
    $target = [IO.Directory]::CreateDirectory((Join-Path $temp.FullName 'target')).FullName
    $outDir = [IO.Directory]::CreateDirectory((Join-Path $target 'out')).FullName
    $metadata = Join-Path $temp.FullName 'metadata.json'
    $log = Join-Path $temp.FullName 'build.jsonl'
    $registry = 'registry+https://github.com/rust-lang/crates.io-index'
    @{ packages = @(
        @{ name = 'glutin_wgl_sys'; version = '0.6.1'; id = 'fixture-id'; source = $registry },
        @{ name = 'ring'; version = '0.17.14'; id = 'ring-id'; source = $registry },
        @{ name = 'libsqlite3-sys'; version = '0.36.0'; id = 'sqlite-id'; source = $registry }
    ) } |
        ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $metadata
    $event = @{ reason = 'build-script-executed'; package_id = 'fixture-id'; out_dir = $outDir } |
        ConvertTo-Json -Compress
    Set-Content -LiteralPath $log -Value $event
    $text = @('WinNT.h', 'Windef.h', 'BaseTsd.h', 'IntSafe.h', 'Wingdi.h') | ForEach-Object { "// From $_" }
    foreach ($name in @('wgl_bindings.rs', 'wgl_extra_bindings.rs')) {
        Set-Content -LiteralPath (Join-Path $outDir $name) -Value $text
    }
    $metadataHash = Hash $metadata
    $observed = Read-WglGeneratedBindings $metadata $log $target $metadataHash
    if ($observed.status -cne 'observed' -or $observed.files.Count -ne 2) { throw 'Expected two bindings.' }
    foreach ($file in $observed.files) {
        if ($file.sha256 -cne (Hash (Join-Path $outDir $file.file))) { throw 'Binding hash mismatch.' }
    }
    $native = @(
        @{ package_id = 'ring-id'; out_dir = $outDir; linked_paths = @("native=$outDir")
            linked_libs = @('static=ring_core_0_17_14_', 'static=ring_core_0_17_14__test') },
        @{ package_id = 'sqlite-id'; out_dir = $outDir; linked_paths = @("native=$outDir")
            linked_libs = @('static=sqlite3') }
    )
    foreach ($name in @('ring_core_0_17_14_.lib', 'ring_core_0_17_14__test.lib', 'sqlite3.lib')) {
        Set-Content -LiteralPath (Join-Path $outDir $name) -Value "synthetic $name"
    }
    $archives = Read-DeclaredNativeArchives $metadata $metadataHash $native $target
    if ($archives.status -cne 'observed' -or $archives.packages.Count -ne 2 -or
        ($archives.packages.cargo_package_id -join '|') -cne 'ring-id|sqlite-id' -or
        ($archives.packages.target_relative_out_dir -join '|') -cne 'out|out') { throw 'Expected both archive sets.' }
    foreach ($file in $archives.packages.archives) {
        $path = Join-Path $outDir $file.file
        if ($file.bytes -ne (Get-Item -LiteralPath $path).Length -or $file.sha256 -cne (Hash $path)) {
            throw 'Native archive identity mismatch.'
        }
    }
    $native[1].linked_paths = @()
    $partial = Read-DeclaredNativeArchives $metadata $metadataHash (@($native[0]) + $native) $target
    if ($partial.status -cne 'partial' -or $partial.packages[0].status -cne 'refused' -or
        $partial.packages[1].reason -cne 'native_out_dir_not_declared_for_search') {
        throw 'Ambiguous or incomplete native archive evidence was accepted.'
    }
    $linker = Join-Path $outDir 'synthetic-link.exe'
    $pathsFile = Join-Path $temp.FullName 'where.txt'
    Set-Content -LiteralPath $linker -Value 'Synthetic linker file for hash and path resolution checks.'
    Set-Content -LiteralPath $pathsFile -Value $linker
    $selected = Read-SelectedLinkerIdentity $pathsFile $linker
    if ($selected.sha256 -cne (Hash $linker) -or $selected.bytes -ne (Get-Item -LiteralPath $linker).Length) {
        throw 'Selected tool identity mismatch.'
    }
    $unexpected = Read-SelectedLinkerIdentity $pathsFile (Join-Path $outDir 'wrong.exe')
    if ($unexpected.status -cne 'refused' -or $unexpected.Contains('sha256')) { throw 'Accepted a mismatched linker.' }

    Add-Content -LiteralPath $log -Value $event
    $duplicate = Read-WglGeneratedBindings $metadata $log $target $metadataHash
    if ($duplicate.status -cne 'refused' -or $duplicate.reason -cne 'cargo_build_script_message_missing_or_ambiguous') {
        throw 'Duplicate Cargo records were accepted.'
    }
    [IO.File]::Open($log, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None).Dispose()
} finally { $temp.Delete($true) }

Write-Output 'Windows evidence checks passed: CRT priority, WGL files, linker and native archives; ambiguity and close controls.'
