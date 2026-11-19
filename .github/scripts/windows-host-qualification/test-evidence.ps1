$ErrorActionPreference = 'Stop'
$syntax = @{}
foreach ($name in @('qualify.ps1', 'gui-fixture.ps1')) {
    $tokens = $null
    $errors = $null
    $syntax[$name] = [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
foreach ($name in @('Hash', 'Read-WglGeneratedBindings')) {
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
    @{ packages = @(@{ name = 'glutin_wgl_sys'; version = '0.6.1'; id = 'fixture-id'
        source = 'registry+https://github.com/rust-lang/crates.io-index' }) } |
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
    Add-Content -LiteralPath $log -Value $event
    $duplicate = Read-WglGeneratedBindings $metadata $log $target $metadataHash
    if ($duplicate.status -cne 'refused' -or $duplicate.reason -cne 'cargo_build_script_message_missing_or_ambiguous') {
        throw 'Duplicate Cargo records were accepted.'
    }
    [IO.File]::Open($log, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None).Dispose()
} finally { $temp.Delete($true) }

Write-Output 'Windows evidence checks passed: CRT priority, exact WGL files and duplicate refusal.'
