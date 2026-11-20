# Consume only artifacts from the successful joint Cargo build with a fresh target.
function Get-ProductMapFile([string]$Path, [long]$Limit) {
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $item.Length -le 0 -or $item.Length -gt $Limit) { throw 'invalid_product_map_file' }
    $item
}

function Get-ProductMapPe([string]$Path) {
    $file = Get-ProductMapFile $Path 128MB
    $stream = [IO.File]::Open($Path, 'Open', 'Read', 'Read')
    try {
        if ($stream.Length -ne $file.Length -or $file.Length -lt 120) { throw 'invalid_pe_length' }
        $reader = [IO.BinaryReader]::new($stream, [Text.Encoding]::ASCII, $true)
        try {
            if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'missing_mz' }
            $stream.Position = 0x3c
            $offset = $reader.ReadInt32()
            if ($offset -lt 64 -or $offset -gt $file.Length - 56) { throw 'invalid_pe_offset' }
            $stream.Position = $offset
            if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'expected_amd64_pe' }
            $stream.Position = $offset + 8
            $stamp = $reader.ReadUInt32()
            $stream.Position = $offset + 24
            if ($reader.ReadUInt16() -ne 0x20b) { throw 'expected_pe32_plus' }
            $stream.Position = $offset + 48
            $imageBase = $reader.ReadUInt64()
        } finally { $reader.Dispose() }
        $stream.Position = 0
        $digest = [Security.Cryptography.SHA256]::HashData($stream)
        if ($stream.Position -ne $file.Length -or $stream.Length -ne $file.Length) { throw 'pe_changed' }
        @{ bytes = $file.Length; sha256 = [Convert]::ToHexString($digest).ToLowerInvariant()
           stamp = $stamp; image_base = $imageBase }
    } finally { $stream.Dispose() }
}

function Get-ProductMapOrigin([string]$Label) {
    $archive = (($Label -split ':(?![\\/])', 2)[0].Trim('"') -replace '^.*[\\/]', '')
    switch -Regex ($archive) {
        '^(?:lib)?(?<crate>cueglint(?:[-_](?:cli|core|local|providers))?)(?:-[0-9a-f]{16})?(?:\.rlib|\.[a-z0-9_.-]+\.rcgu\.(?:o|obj))?$' {
            return $Matches['crate'].ToLowerInvariant().Replace('_', '-')
        }
        '^libglutin_wgl_sys-[0-9a-f]{16}(?:\.rlib)?$' { return 'wgl-rlib' }
        '^ring_core_0_17_14__test(?:\.lib)?$' { return 'ring-test-archive' }
        '^ring_core_0_17_14_(?:\.lib)?$' { return 'ring-core-archive' }
        '^libring-[0-9a-f]{16}(?:\.rlib)?$' { return 'ring-rlib' }
        '^sqlite3(?:\.lib)?$' { return 'sqlite-archive' }
        '^liblibsqlite3_sys-[0-9a-f]{16}(?:\.rlib)?$' { return 'sqlite-rlib' }
        '^windows\.0\.52\.0(?:\.lib)?$' { return 'windows-rust-import-0.52' }
        '^windows\.0\.53\.0(?:\.lib)?$' { return 'windows-rust-import-0.53' }
        '^lib(?:std|core|alloc|compiler_builtins|panic_abort|panic_unwind|unwind|std_detect|rustc_demangle|rustc_std_workspace_[a-z_]+)-[0-9a-f]{16}(?:\.rlib)?$' {
            return 'rust-toolchain-name'
        }
        '^(?:lib)?(?:msvcrt|msvcrtd|cmt|cmtd|vcruntime(?:140)?d?|cpmt|cpmtd|msvcprtd?|oldnames|delayimp)(?:\.lib)?$' {
            return 'msvc-crt-name'
        }
        '^(?:(?:lib)?ucrtd?(?:\.lib)?|api-ms-win-crt-[a-z0-9_.-]+\.dll)$' { return 'ucrt-name' }
        '^(?:advapi32|bcrypt(?:primitives)?|combase|d3d12|d3dcompiler(?:_47)?|dwmapi|dxgi|gdi32|imm32|kernel32|ntdll|ole32|oleaut32|opengl32|setupapi|shell32|shlwapi|uiautomationcore|user32|uxtheme|ws2_32)(?:\.lib)?$' {
            return 'windows-import-name'
        }
    }
    'unclassified'
}

function Read-ProductMap([string]$Path, [string]$Stem, $Pe) {
    $file = Get-ProductMapFile $Path 32MB
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -ne $file.Length) { throw 'map_size_changed' }
    $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes).TrimStart([char]0xfeff)
    $groups = @{}
    $section = 'header'
    $module = $null
    $stamp = $null
    $imageBase = $null
    $rows = 0
    foreach ($line in $text.Split([char]10)) {
        if ($line.Length -gt 64KB) { throw 'map_line_limit' }
        $line = $line.Trim()
        if (-not $line) { continue }
        if ($null -eq $module) {
            $module = $line
            if ($module -cne $Stem -and $module -cne "$Stem.exe") { throw 'map_module_mismatch' }
            continue
        }
        if ($line -match '^Address[ \t]+Publics by Value[ \t]+Rva\+Base[ \t]+Lib:Object$') {
            if ($section -ne 'header') { throw 'duplicate_map_table' }
            $section = 'publics'
            continue
        }
        if ($section -eq 'header') {
            if ($line -match '^Timestamp is[ \t]+([0-9a-f]{8})(?:[ \t]+\([^)]*\))?$') {
                if ($null -ne $stamp) { throw 'duplicate_map_stamp' }
                $stamp = [Convert]::ToUInt32($Matches[1], 16)
            } elseif ($line -match '^Preferred load address is[ \t]+([0-9a-f]{8,16})$') {
                if ($null -ne $imageBase) { throw 'duplicate_map_base' }
                $imageBase = [Convert]::ToUInt64($Matches[1], 16)
            }
            continue
        }
        if ($section -ne 'publics') { continue }
        if ($line -match '^entry point at[ \t]+[0-9a-f]{4}:[0-9a-f]{8,16}$') {
            $section = 'after_publics'
            continue
        }
        if ($line -notmatch '^[0-9a-f]{4}:[0-9a-f]{8,16}[ \t]+\S+[ \t]+[0-9a-f]{8,16}(?:[ \t]+(?<origin>.*))?$') {
            throw 'invalid_map_public_row'
        }
        if (++$rows -gt 262144) { throw 'map_public_row_limit' }
        $label = ([string]$Matches['origin'] -replace '^(?:[fia]{1,3}[ \t]+){0,3}', '').Trim()
        $alias = Get-ProductMapOrigin $label
        if (-not $groups.ContainsKey($alias)) {
            $groups[$alias] = @{ rows = 0; labels = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal) }
        }
        $groups[$alias].rows++
        [void]$groups[$alias].labels.Add($label)
    }
    if ($section -ne 'after_publics' -or $rows -eq 0 -or
        $null -eq $stamp -or $stamp -ne $Pe.stamp -or
        $null -eq $imageBase -or $imageBase -ne $Pe.image_base) { throw 'map_pe_header_mismatch' }
    $aliases = foreach ($alias in ($groups.Keys | Sort-Object)) {
        $labels = [string[]]@($groups[$alias].labels)
        [Array]::Sort($labels, [StringComparer]::Ordinal)
        $digest = [Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($labels -join [char]10))
        [ordered]@{ alias = $alias; public_rows = $groups[$alias].rows; distinct_labels = $labels.Length
                    labels_sha256 = [Convert]::ToHexString($digest).ToLowerInvariant() }
    }
    [ordered]@{
        bytes = $file.Length
        sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
        public_rows = $rows
        unclassified_public_rows = if ($groups.ContainsKey('unclassified')) { $groups['unclassified'].rows } else { 0 }
        origins = @($aliases)
    }
}

function Get-ProductLinkWrapperRefusals([string]$Path) {
    $item = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
    if (-not $item -or $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $item.Length -gt 32MB) { return }
    $known = @(
        'missing rustc flag value', 'ambiguous rustc flag', 'unexpected product crate name or type',
        'custom product output is unsupported', 'ambiguous rustc artifact suffix', 'existing linker map flag',
        'invalid rustc artifact suffix', 'missing map or artifact path',
        'reparse point in evidence path', 'symlink in evidence path', 'missing rustc executable',
        'could not launch rustc', 'missing private Cargo target', 'invalid private Cargo target',
        'unexpected rustc output directory', 'could not create private map directory', 'invalid private map directory',
        'duplicate product link', 'product link output path is already occupied', 'product link outputs missing',
        'could not complete product link marker'
    )
    $matched = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($line in [IO.File]::ReadLines($item.FullName)) {
        if ($line -cmatch '^product link map wrapper: ([a-z ]{1,80})$' -and $Matches[1] -cin $known) {
            [void]$matched.Add($Matches[1])
        }
    }
    $matched | Sort-Object
}

function Get-ProductLinkMapReport([string]$TargetRoot, [object[]]$Artifacts) {
    $report = [ordered]@{
        schema_version = 1; status = 'refused'; reason = 'invalid_cargo_artifacts'
        scope = 'Retained public-symbol source labels only. Aliases classify spellings. Archive searches, selected or discarded members and filesystem origins are outside this receipt.'
        binaries = @()
    }
    try {
        $target = 'x86_64-pc-windows-msvc'
        if ($Artifacts.Count -ne 2 -or -not [IO.Path]::IsPathFullyQualified($TargetRoot)) { return $report }
        $root = [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetFullPath($TargetRoot))
        $platform = Join-Path $root $target
        $release = Join-Path $platform 'release'
        $deps = Join-Path $release 'deps'
        $maps = Join-Path $root 'cueglint-link-maps'
        foreach ($path in @($root, $platform, $release, $deps, $maps)) {
            $item = Get-Item -LiteralPath $path -ErrorAction Stop
            if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                return $report
            }
        }
        foreach ($name in @('cueglint', 'cueglint-cli')) {
            $report.reason = 'invalid_cargo_artifacts'
            $artifact = @($Artifacts | Where-Object { $_.target.name -ceq $name })
            $executable = Join-Path $release "$name.exe"
            if ($artifact.Count -ne 1 -or $artifact[0].reason -cne 'compiler-artifact' -or
                $artifact[0].fresh -isnot [bool] -or $artifact[0].fresh -or
                (@($artifact[0].target.kind) -join '|') -cne 'bin' -or
                (@($artifact[0].target.crate_types) -join '|') -cne 'bin' -or
                $artifact[0].executable -isnot [string] -or $artifact[0].executable -cne $executable) { return $report }
            $report.reason = 'missing_or_invalid_link_marker'
            $marker = Get-ProductMapFile (Join-Path $maps "$name.linked") 64
            $crate = $name.Replace('-', '_')
            $stem = [IO.File]::ReadAllText($marker.FullName, [Text.Encoding]::ASCII).TrimEnd([char]13, [char]10)
            if ($stem -cnotmatch ('^' + $crate + '(?:-[0-9a-f]{16})?$')) { return $report }
            $report.reason = 'cargo_pe_identity_mismatch'
            $pe = Get-ProductMapPe $executable
            $internalPe = Get-ProductMapPe (Join-Path $deps "$stem.exe")
            if ($pe.bytes -ne $internalPe.bytes -or $pe.sha256 -cne $internalPe.sha256) { return $report }
            $report.reason = 'missing_or_invalid_pe_map'
            $map = Read-ProductMap (Join-Path $maps "$name.map") $stem $pe
            $report.binaries += [ordered]@{ name = $name; pe = @{ bytes = $pe.bytes; sha256 = $pe.sha256 }; map = $map }
        }
        $report.status = 'observed'
        $report.reason = 'fresh_cargo_pe_public_origins'
    } catch {
        # File and parse exceptions may carry private paths.
    }
    $report
}
