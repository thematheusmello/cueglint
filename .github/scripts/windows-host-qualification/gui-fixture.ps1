# One owned primary-viewport fixture; this never captures the desktop or sends a question.
function Read-GuiFixtureReceipt([string]$Stdout, [string]$Stderr, [string]$Png) {
    $fixture = 'native-v2-dashboard'
    $events = @($Stdout -split '\r?\n' | ForEach-Object {
        try { $_ | ConvertFrom-Json -AsHashtable -ErrorAction Stop } catch { }
    } | Where-Object { $_.event -eq 'native_screenshot_saved' })
    if ($events.Count -ne 1 -or $events[0].fixture -cne $fixture) { throw 'Expected one saved GUI fixture receipt.' }
    $event = $events[0]
    $id = $event.request_id
    $font = $event.fonts.identity.font_set_id
    if ($id -isnot [string] -or $id -notmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' -or
        $event.fonts.state -cne 'ready' -or $font -isnot [string] -or $font -notmatch '^[0-9a-f]{64}$') { throw 'GUI request/fonts were not identified and ready.' }
    $trace = @($Stderr -split '\r?\n' | ForEach-Object {
        try { $_ | ConvertFrom-Json -AsHashtable -ErrorAction Stop } catch { }
    } | Where-Object { $_.fixture -ceq $fixture -and $_.viewport -ceq 'root' })
    foreach ($stage in @('requested', 'copy_submitted', 'readback_ready')) {
        $kind = if ($stage -eq 'requested') { 'native_screenshot_stage' } else { 'native_screenshot_transport' }
        $stageEvents = @($trace | Where-Object { $_.request_id -ceq $id -and $_.event -ceq $kind -and $_.stage -ceq $stage })
        if ($stageEvents.Count -ne 1) { throw "Missing unique GUI $stage trace." }
        if ($stage -eq 'requested' -and ($stageEvents[0].font_state.state -cne 'ready' -or
            $stageEvents[0].font_state.identity.font_set_id -cne $font)) { throw 'Requested and saved GUI fonts differ.' }
    }
    if (@($trace | Where-Object { $_.event -eq 'native_screenshot_transport' -and
        $_.stage -in @('map_failed','format_unsupported','delivery_failed','queue_overflow','viewport_dropped') }).Count) {
        throw 'GUI screenshot transport reported a terminal failure.'
    }
    if ($event.path -isnot [string] -or -not $event.path -or -not [string]::Equals([IO.Path]::GetFullPath($event.path), [IO.Path]::GetFullPath($Png), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'GUI screenshot path differs from the owned output.'
    }
    foreach ($field in @('pixels_per_point', 'zoom_factor')) {
        if ($event[$field] -isnot [ValueType] -or -not [double]::IsFinite([double]$event[$field]) -or [double]$event[$field] -le 0) { throw 'Invalid GUI scale.' }
    }
    if (-not $event.adapter.backend -or -not $event.adapter.name) { throw 'GUI adapter identity is missing.' }
    $file = Get-Item -LiteralPath $Png
    if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -lt 24 -or $file.Length -gt 16MB) { throw 'Invalid or oversized GUI PNG.' }
    $stream = [IO.File]::OpenRead($Png)
    try {
        $header = [byte[]]::new(24)
        if ($stream.Read($header, 0, 24) -ne 24 -or [Convert]::ToHexString($header[0..15]) -cne '89504E470D0A1A0A0000000D49484452') { throw 'Invalid GUI PNG header.' }
    } finally { $stream.Dispose() }
    $width = [BitConverter]::ToUInt32([byte[]]@($header[19],$header[18],$header[17],$header[16]), 0)
    $height = [BitConverter]::ToUInt32([byte[]]@($header[23],$header[22],$header[21],$header[20]), 0)
    if ($width -lt 1 -or $height -lt 1 -or $width -gt 4096 -or $height -gt 4096 -or
        $event.image.width -ne $width -or $event.image.height -ne $height) { throw 'GUI PNG dimensions differ from the receipt or exceed bounds.' }
    $event['verified_png_sha256'] = (Get-FileHash -LiteralPath $Png -Algorithm SHA256).Hash.ToLowerInvariant()
    return $event
}

function Invoke-GuiFixture([string]$Executable, [string]$Out, [Collections.IDictionary]$State) {
    $root = Join-Path $Out 'gui-fixture'
    if (Test-Path -LiteralPath $root) { throw 'Use a fresh GUI fixture directory.' }
    $null = New-Item -ItemType Directory -Path $root
    foreach ($name in @('data','models','temp')) { $null = New-Item -ItemType Directory -Path (Join-Path $root $name) }
    $png = Join-Path $root 'native-v2-dashboard.png'
    $State['executable_sha256'] = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $State['fixture_sha256'] = (Get-FileHash -LiteralPath 'ui/fixtures/native-v2.json' -Algorithm SHA256).Hash.ToLowerInvariant()
    $State['deadline_ms'] = 30000
    $State['log_poll_ms'] = 25
    $State['log_limit_bytes'] = 1MB
    $State['png_limit_bytes'] = 16MB
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.WorkingDirectory = $root; $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $start.Environment['CUEGLINT_DATA_DIR'] = Join-Path $root 'data'
    $start.Environment['CUEGLINT_MODEL_DIR'] = Join-Path $root 'models'
    $start.Environment['TEMP'] = Join-Path $root 'temp'; $start.Environment['TMP'] = Join-Path $root 'temp'
    foreach ($argument in @('--fixture','native-v2-dashboard','--screenshot',$png)) { $start.ArgumentList.Add($argument) }
    $process = $null; $streams = @(); $copies = @(); $failure = $null; $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        foreach ($name in @('stdout','stderr')) {
            $streams += [IO.File]::Open((Join-Path $Out "gui-fixture.$name.txt"), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        }
        $process = [Diagnostics.Process]::Start($start)
        $State['started'] = $true; $State['pid'] = $process.Id
        $copies = @($process.StandardOutput.BaseStream.CopyToAsync($streams[0]), $process.StandardError.BaseStream.CopyToAsync($streams[1]))
        while (-not $process.WaitForExit(25)) {
            if ($timer.ElapsedMilliseconds -ge 30000) { throw 'GUI fixture deadline exceeded; no retry.' }
            if (@($streams | Where-Object { $_.Length -gt 1MB }).Count) { throw 'GUI fixture log limit exceeded.' }
        }
        $State['exit_code'] = $process.ExitCode
        if (-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]$copies, 2000)) { throw 'GUI fixture streams did not close.' }
        foreach ($stream in $streams) { $stream.Flush(); if ($stream.Length -gt 1MB) { throw 'GUI fixture log limit exceeded.' } }
        if ($process.ExitCode -ne 0) { throw 'GUI fixture exited unsuccessfully.' }
    } catch { $failure = $_ } finally {
        if ($process) {
            try {
                if (-not $process.HasExited) { $State['tree_kill_requested'] = $true; $process.Kill($true) }
                $State['root_exit_confirmed'] = $process.WaitForExit(5000)
                if (-not $State['root_exit_confirmed']) { throw 'GUI fixture cleanup did not confirm exit.' }
                $State['exit_code'] = $process.ExitCode
                if ($copies.Count -and -not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]$copies, 2000)) { throw 'GUI fixture stream cleanup exceeded its bound.' }
            } catch { $State['cleanup_failure'] = $_.Exception.Message; if (-not $failure) { $failure = $_ } }
            $process.Dispose()
        }
        foreach ($stream in $streams) { $stream.Dispose() }
        $State['process_elapsed_ms'] = $timer.ElapsedMilliseconds
    }
    if ($failure) { $State['failure'] = $failure.Exception.Message; throw $failure }
    try {
        $receipt = Read-GuiFixtureReceipt ([IO.File]::ReadAllText((Join-Path $Out 'gui-fixture.stdout.txt'))) ([IO.File]::ReadAllText((Join-Path $Out 'gui-fixture.stderr.txt'))) $png
        $State['receipt'] = $receipt
        Add-Type -AssemblyName System.Drawing
        $image = [Drawing.Image]::FromFile($png)
        try {
            if ($image.Width -ne $receipt.image.width -or $image.Height -ne $receipt.image.height) { throw 'Decoded GUI PNG differs from its receipt.' }
        } finally { $image.Dispose() }
        $State['png_decoded'] = $true
        if ((Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $State['executable_sha256'] -or
            (Get-FileHash -LiteralPath 'ui/fixtures/native-v2.json' -Algorithm SHA256).Hash.ToLowerInvariant() -cne $State['fixture_sha256']) { throw 'GUI source/binary identity changed.' }
        if (@(Get-ChildItem -LiteralPath (Join-Path $root 'data') -Force).Count -or @(Get-ChildItem -LiteralPath (Join-Path $root 'models') -Force).Count) { throw 'GUI fixture wrote data/model files.' }
        $State['passed'] = $true
    } catch { $State['failure'] = $_.Exception.Message; throw } finally { $State['total_elapsed_ms'] = $timer.ElapsedMilliseconds }
}
