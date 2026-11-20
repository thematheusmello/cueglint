$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'product-link-maps.ps1')

function Assert-ProductMap($Condition, [string]$Case) {
    if (-not $Condition) { throw "Product map control failed: $Case" }
}

$root = Join-Path ([IO.Path]::GetTempPath()) ('cueglint-map-control-' + [Guid]::NewGuid().ToString('N'))
$release = Join-Path $root 'x86_64-pc-windows-msvc/release'
$deps = Join-Path $release 'deps'
$maps = Join-Path $root 'cueglint-link-maps'
[void][IO.Directory]::CreateDirectory($deps)
[void][IO.Directory]::CreateDirectory($maps)
try {
    $pe = [byte[]]::new(144)
    [BitConverter]::GetBytes([uint16]0x5a4d).CopyTo($pe, 0)
    [BitConverter]::GetBytes([int32]64).CopyTo($pe, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($pe, 64)
    [BitConverter]::GetBytes([uint16]0x8664).CopyTo($pe, 68)
    [BitConverter]::GetBytes([uint32]0x12345678).CopyTo($pe, 72)
    [BitConverter]::GetBytes([uint16]0x20b).CopyTo($pe, 88)
    [BitConverter]::GetBytes([uint64]0x140000000).CopyTo($pe, 112)
    $artifactRecords = foreach ($name in @('cueglint', 'cueglint-cli')) {
        $stem = if ($name -ceq 'cueglint') { 'cueglint-0123456789abcdef' } else { 'cueglint_cli' }
        $exe = Join-Path $release "$name.exe"
        $pe[$pe.Length - 1]++
        [IO.File]::WriteAllBytes($exe, $pe)
        [IO.File]::WriteAllBytes((Join-Path $deps "$stem.exe"), $pe)
        Set-Content -Encoding ascii -LiteralPath (Join-Path $maps "$name.linked") -Value $stem
        @"
 $stem.exe

 Timestamp is 12345678 (synthetic)
 Preferred load address is 0000000140000000

  Address     Publics by Value          Rva+Base         Lib:Object
 0001:00000010      private_symbol_1    0000000140001010  f   $stem.cgu.0.rcgu.o
 0001:00000020      private_symbol_2    0000000140001020  f   libglutin_wgl_sys-0123456789abcdef.rlib:binding.obj
 0001:00000030      private_symbol_3    0000000140001030  f   C:\private build\ring_core_0_17_14_.lib:core.obj
 0001:00000031      private_symbol_4    0000000140001031  f i ring_core_0_17_14_:core2.obj
 0001:00000040      private_symbol_5    0000000140001040  f   liblibsqlite3_sys-0123456789abcdef.rlib:sqlite3.obj
 0001:00000050      private_symbol_6    0000000140001050  i   windows.0.52.0:KERNEL32.dll
 0001:00000060      private_symbol_7    0000000140001060      libstd-0123456789abcdef.rlib:std.obj
 0001:00000070      private_symbol_8    0000000140001070      msvcrt:crt.obj
 0001:00000080      private_symbol_9    0000000140001080      C:\some other folder\other.lib:other.obj
 0001:00000090      private_symbol_10   0000000140001090      C:\some other folder\other.lib:other.obj

 entry point at 0001:00000010

 Static symbols
 0001:000000a0      unused_private_row  00000001400010a0      ring_core_0_17_14__test:unused.obj
"@ | Set-Content -Encoding ascii -LiteralPath (Join-Path $maps "$name.map")
        [pscustomobject]@{
            reason = 'compiler-artifact'; target = @{ name = $name; kind = @('bin'); crate_types = @('bin') }
            executable = $exe; fresh = $false
        }
    }

    $report = Get-ProductLinkMapReport $root $artifactRecords
    Assert-ProductMap ($report.status -ceq 'observed' -and $report.binaries.Count -eq 2) $report.reason
    foreach ($binary in $report.binaries) {
        $alias = @($binary.map.origins | Where-Object { $_.alias -ceq 'ring-core-archive' })
        Assert-ProductMap ($alias.Count -eq 1 -and $alias[0].public_rows -eq 2 -and $alias[0].distinct_labels -eq 2) 'ring labels'
        Assert-ProductMap ($binary.map.public_rows -eq 10 -and $binary.map.unclassified_public_rows -eq 2) 'row totals'
        Assert-ProductMap ($binary.map.origins.alias -contains $binary.name -and
            $binary.map.origins.alias -contains 'wgl-rlib' -and
            $binary.map.origins.alias -contains 'sqlite-rlib' -and
            $binary.map.origins.alias -contains 'windows-rust-import-0.52' -and
            $binary.map.origins.alias -contains 'rust-toolchain-name' -and
            $binary.map.origins.alias -contains 'msvc-crt-name' -and
            $binary.map.origins.alias -notcontains 'ring-test-archive') 'public origin aliases'
        $hash = (Get-FileHash -LiteralPath (Join-Path $release "$($binary.name).exe") -Algorithm SHA256).Hash
        Assert-ProductMap ($binary.pe.sha256 -ceq $hash.ToLowerInvariant()) 'cargo PE hash'
    }
    $json = ConvertTo-Json -InputObject $report -Depth 10
    Assert-ProductMap ($json -notmatch 'private_symbol|other\.obj|binding\.obj|private build|some other folder|[A-Fa-f]:\\') 'public data boundary'
    Assert-ProductMap ($report.binaries[0].pe.sha256 -cne $report.binaries[1].pe.sha256) 'separate PEs'

    $artifactRecords[0].fresh = $true
    Assert-ProductMap ((Get-ProductLinkMapReport $root $artifactRecords).status -ceq 'refused') 'cached artifact'
    $artifactRecords[0].fresh = $false
    Assert-ProductMap ((Get-ProductLinkMapReport $root @($artifactRecords[0], $artifactRecords[0])).status -ceq 'refused') 'duplicate executable'

    $cliMarker = Join-Path $maps 'cueglint-cli.linked'
    Set-Content -Encoding ascii -LiteralPath $cliMarker -Value 'cueglint-cli'
    Assert-ProductMap ((Get-ProductLinkMapReport $root $artifactRecords).reason -ceq 'missing_or_invalid_link_marker') 'Cargo name is not the Rust stem'
    Set-Content -Encoding ascii -LiteralPath $cliMarker -Value 'cueglint_cli'

    $map = Join-Path $maps 'cueglint.map'
    $original = Get-Content -LiteralPath $map -Raw
    foreach ($case in @(
        @{ data = $original -replace 'Timestamp is 12345678', 'Timestamp is 23456789'; reason = 'map_pe_stamp_mismatch' }
        @{ data = $original -replace 'Preferred load address is 0000000140000000', 'Preferred load address is 0000000240000000'; reason = 'map_pe_base_mismatch' }
        @{ data = $original -replace '(?m)^ entry point at.*', ' absent entry point'; reason = 'invalid_map_public_row' }
        @{ data = $original -replace 'cueglint-0123456789abcdef.exe', 'cueglint_cli-0123456789abcdef.exe'; reason = 'map_module_mismatch' }
    )) {
        Set-Content -LiteralPath $map -Encoding ascii -Value $case.data
        $bad = Get-ProductLinkMapReport $root $artifactRecords
        Assert-ProductMap ($bad.reason -ceq 'missing_or_invalid_pe_map' -and
            $bad.refusals[0].reason -ceq $case.reason) "map mismatch: expected $($case.reason), observed $($bad.refusals[0].reason)"
    }
    [IO.File]::WriteAllBytes($map, [byte[]]@(0xff, 0xfe, 0xff))
    $badEncoding = Get-ProductLinkMapReport $root $artifactRecords
    Assert-ProductMap ($badEncoding.refusals[0].reason -ceq 'invalid_map_encoding' -and
        $badEncoding.refusals[0].map.bytes -eq 3) 'invalid map encoding'
    # A malformed desktop map must retain its file identity and the independent, valid CLI map.
    $bad = $original -replace '0001:00000010      private_symbol_1', 'PRIVATE/fixture-content'
    Set-Content -LiteralPath $map -Encoding ascii -Value $bad
    $mixed = Get-ProductLinkMapReport $root $artifactRecords
    $refusal = $mixed.refusals[0]
    Assert-ProductMap ($mixed.status -ceq 'refused' -and $mixed.refusals.Count -eq 1 -and
        $mixed.binaries.Count -eq 1 -and $mixed.binaries[0].name -ceq 'cueglint-cli' -and
        $mixed.binaries[0].map.public_rows -eq 10 -and $refusal.name -ceq 'cueglint' -and
        $refusal.stage -ceq 'missing_or_invalid_pe_map' -and $refusal.reason -ceq 'invalid_map_public_row' -and
        $refusal.pe.sha256 -ceq $report.binaries[0].pe.sha256 -and
        $refusal.map.sha256 -ceq (Get-FileHash $map).Hash.ToLowerInvariant() -and
        $refusal.map.bytes -eq (Get-Item $map).Length) 'independent binding and malformed map identity'
    Assert-ProductMap ((ConvertTo-Json $mixed -Depth 10) -notmatch 'PRIVATE/fixture-content|private_symbol|some other folder') 'map data excluded on refusal'

    # Match the observed desktop map's byte count using only original synthetic padding.
    [IO.File]::WriteAllText($map, $original + [char]10, [Text.Encoding]::ASCII)
    $padding = [Text.Encoding]::ASCII.GetBytes((' ' * 4095) + [char]10)
    $large = [IO.File]::Open($map, 'Append', 'Write', 'None')
    try {
        while ($large.Length -lt 33654718) {
            $large.Write($padding, 0, [int][Math]::Min([long]$padding.Length, 33654718 - $large.Length))
        }
    } finally { $large.Dispose() }
    $observed = Get-ProductLinkMapReport $root $artifactRecords
    Assert-ProductMap ($observed.status -ceq 'observed' -and $observed.binaries.Count -eq 2 -and
        $observed.binaries[0].map.bytes -eq 33654718 -and $observed.binaries[0].map.public_rows -eq 10 -and
        $observed.binaries[0].map.sha256 -ceq (Get-FileHash $map).Hash.ToLowerInvariant()) 'full-size desktop map'

    [IO.File]::WriteAllText($map, $original + [char]10 + ('x' * (64KB + 1)), [Text.Encoding]::UTF8)
    $longLine = Get-ProductLinkMapReport $root $artifactRecords
    Assert-ProductMap ($longLine.refusals.Count -eq 1 -and
        $longLine.refusals[0].reason -ceq 'map_line_limit' -and
        $longLine.binaries.Count -eq 1) 'line bound after the public table'

    $oversized = [IO.File]::OpenWrite($map)
    try { $oversized.SetLength(64MB + 1) } finally { $oversized.Dispose() }
    $mixed = Get-ProductLinkMapReport $root $artifactRecords
    Assert-ProductMap ($mixed.refusals.Count -eq 1 -and $mixed.refusals[0].reason -ceq 'file_size_limit' -and
        $mixed.refusals[0].map.bytes -eq (64MB + 1) -and -not $mixed.refusals[0].map.ContainsKey('sha256') -and
        $mixed.binaries.Count -eq 1 -and $mixed.binaries[0].name -ceq 'cueglint-cli') 'oversized map size without read or hash'

    $privateFailure = & {
        function Read-ProductMap { throw 'cannot inspect PRIVATE/fixture-content near private_symbol' }
        Get-ProductLinkMapReport $root $artifactRecords
    }
    Assert-ProductMap ($privateFailure.refusals.Count -eq 2 -and
        ($privateFailure.refusals.reason -join '|') -ceq 'unknown_error|unknown_error' -and
        (ConvertTo-Json $privateFailure -Depth 10) -notmatch 'PRIVATE/fixture-content|private_symbol') 'arbitrary exception redacted for both roles'

    Set-Content -LiteralPath $map -Encoding ascii -Value $original
    $pe[$pe.Length - 1]++
    [IO.File]::WriteAllBytes((Join-Path $deps 'cueglint-0123456789abcdef.exe'), $pe)
    Assert-ProductMap ((Get-ProductLinkMapReport $root $artifactRecords).reason -ceq 'cargo_pe_identity_mismatch') 'mismatched PE'
    $stderr = Join-Path $root 'cargo.stderr.txt'
    Set-Content -Encoding ascii -LiteralPath $stderr -Value @(
        'product link map wrapper: duplicate product link',
        'product link map wrapper: unexpected rustc output directory',
        'product link map wrapper: duplicate product link',
        'product link map wrapper: missing fixture secret',
        'product link map wrapper: could not launch rustc C:\private\example'
    )
    Assert-ProductMap ((@(Get-ProductLinkWrapperRefusals $stderr) -join '|') -ceq
        'duplicate product link|unexpected rustc output directory') 'fixed wrapper refusal labels'
    Assert-ProductMap (@(Get-ProductLinkWrapperRefusals (Join-Path $root 'absent')).Count -eq 0) 'absent build log'
    Write-Output 'Product link map controls passed.'
} finally {
    Remove-Item -LiteralPath $root -Force -Recurse
}
