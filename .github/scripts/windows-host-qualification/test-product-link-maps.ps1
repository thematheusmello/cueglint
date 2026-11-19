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
        $stem = $name.Replace('-', '_') + '-0123456789abcdef'
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

    $map = Join-Path $maps 'cueglint.map'
    $original = Get-Content -LiteralPath $map -Raw
    foreach ($bad in @(
        ($original -replace '12345678', '23456789'),
        ($original -replace 'Preferred load address is 0000000140000000', 'Preferred load address is 0000000240000000'),
        ($original -replace '(?m)^ entry point at.*', ' absent entry point'),
        ($original -replace 'cueglint-0123456789abcdef.exe', 'cueglint_cli-0123456789abcdef.exe')
    )) {
        Set-Content -LiteralPath $map -Encoding ascii -Value $bad
        Assert-ProductMap ((Get-ProductLinkMapReport $root $artifactRecords).reason -ceq 'missing_or_invalid_pe_map') 'map mismatch'
    }
    Set-Content -LiteralPath $map -Encoding ascii -Value $original
    $pe[$pe.Length - 1]++
    [IO.File]::WriteAllBytes((Join-Path $deps 'cueglint-0123456789abcdef.exe'), $pe)
    Assert-ProductMap ((Get-ProductLinkMapReport $root $artifactRecords).reason -ceq 'cargo_pe_identity_mismatch') 'mismatched PE'
    Write-Output 'Product link map controls passed.'
} finally {
    Remove-Item -LiteralPath $root -Force -Recurse
}
