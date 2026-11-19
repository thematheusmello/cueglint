# One owned primary-viewport fixture; this never captures the desktop or sends a question.
function Initialize-GuiDllProbe([Collections.IDictionary]$State) {
    $State['loaded_dlls'] = [ordered]@{
        status = 'unavailable'; reason = 'observer_not_started'
        scope = 'Selected DLL names sampled from the owned fixture root. Version and SHA-256 describe on-disk files.'
        method = 'K32EnumProcessModulesEx (64-bit) and K32GetModuleFileNameExW'
        exhaustive = $false; modules = @(); result_wait_ms = 1500
    }
    if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
        $State.loaded_dlls.reason = 'windows_x64_probe_unavailable'
        return $false
    }
    try {
        if (-not ('CueglintGuiDllProbe' -as [type])) {
            Add-Type -ErrorAction Stop -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

public static class CueglintGuiDllProbe
{
    private const int SampleCount = 24, IntervalMs = 200, SampleWindowMs = 5000;
    private const int HandleLimit = 512, PathChars = 512, RecordLimit = 32, FileWindowMs = 1250;
    private const long FileBytes = 32L * 1024 * 1024, TotalBytes = 96L * 1024 * 1024;
    private const uint WaitTimeout = 258, ListModules64Bit = 2;
    private static readonly Regex SelectedName = new Regex(
        @"^(?:(?<crt>vcruntime140(?:_1|_threads)?|msvcp140(?:_[12]|_atomic_wait|_codecvt_ids)?|vccorlib140|concrt140|vcomp140|msvcp_win|msvcrt|ucrtbase|api-ms-win-crt-[a-z0-9-]+)|d3d(?:9|10|10_1|11|11on12|12(?:core|sdklayers)?|compiler_[0-9]+)|dxgi(?:debug)?|dxcore|dxcompiler|dxil|dwrite(?:core)?|vulkan-1|opengl32)\.dll$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr process, uint milliseconds);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool K32EnumProcessModulesEx(IntPtr process, [Out] IntPtr[] modules,
        int size, out uint required, uint filter);

    [DllImport("kernel32.dll", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern uint K32GetModuleFileNameExW(IntPtr process, IntPtr module, StringBuilder path, int size);

    public static Dictionary<string, object> Limits() => new Dictionary<string, object> {
        ["samples"] = SampleCount, ["sample_interval_ms"] = IntervalMs, ["sample_window_ms"] = SampleWindowMs,
        ["handles_per_sample"] = HandleLimit, ["path_chars"] = PathChars, ["records"] = RecordLimit,
        ["identity_window_ms"] = FileWindowMs, ["bytes_per_hash"] = FileBytes, ["total_hash_bytes"] = TotalBytes
    };

    public static Task<Dictionary<string, object>> Start(SafeProcessHandle process, CancellationToken cancel)
    {
        // Retain the exact Start() handle across fixture cleanup, without resolving a PID.
        bool retained = false;
        process.DangerousAddRef(ref retained);
        if (!retained) throw new InvalidOperationException("Could not retain the owned process handle.");
        try {
            IntPtr handle = process.DangerousGetHandle();
            return Task.Run(() => {
                try { return Observe(handle, cancel); }
                finally { process.DangerousRelease(); }
            });
        } catch {
            process.DangerousRelease();
            throw;
        }
    }

    private static Dictionary<string, object> Observe(IntPtr process, CancellationToken cancel)
    {
        var timer = Stopwatch.StartNew();
        var handles = new IntPtr[HandleLimit];
        int bufferSize = handles.Length * IntPtr.Size;
        var nameBuffer = new StringBuilder(PathChars);
        var paths = new SortedSet<string>(StringComparer.OrdinalIgnoreCase);
        int attempts = 0, nonempty = 0, errors = 0, incomplete = 0, lastError = 0;
        string stop = null;
        for (int i = 0; i < SampleCount && timer.ElapsedMilliseconds < SampleWindowMs && !cancel.IsCancellationRequested; i++) {
            uint interval = (uint)Math.Min(IntervalMs, Math.Max(0, SampleWindowMs - timer.ElapsedMilliseconds));
            uint wait = WaitForSingleObject(process, i == 0 ? 0u : interval);
            if (wait == 0) { stop = "owned_process_exited"; break; }
            if (wait != WaitTimeout) {
                errors++;
                lastError = Marshal.GetLastWin32Error();
                stop = "owned_process_wait_failed";
                break;
            }
            if (timer.ElapsedMilliseconds >= SampleWindowMs || cancel.IsCancellationRequested) break;
            attempts++;
            if (!K32EnumProcessModulesEx(process, handles, bufferSize, out uint required, ListModules64Bit)) {
                errors++;
                lastError = Marshal.GetLastWin32Error();
                continue;
            }
            if (required == 0) continue;
            nonempty++;
            if (required > bufferSize) incomplete++;
            int count = Math.Min(handles.Length, (int)(required / IntPtr.Size));
            for (int j = 0; j < count; j++) {
                if (timer.ElapsedMilliseconds >= SampleWindowMs || cancel.IsCancellationRequested) {
                    incomplete++;
                    break;
                }
                if (handles[j] == IntPtr.Zero) { incomplete++; continue; }
                nameBuffer.Clear();
                uint length = K32GetModuleFileNameExW(process, handles[j], nameBuffer, PathChars);
                if (length == 0 || length >= PathChars - 1) { incomplete++; continue; }
                string path = nameBuffer.ToString();
                if (!Path.IsPathFullyQualified(path) || Array.Exists(path.ToCharArray(), char.IsControl)) {
                    incomplete++;
                    continue;
                }
                path = Path.GetFullPath(path);
                if (path.Length >= PathChars) { incomplete++; continue; }
                if (!SelectedName.IsMatch(Path.GetFileName(path))) continue;
                if (paths.Count < RecordLimit) paths.Add(path);
                else if (!paths.Contains(path)) incomplete++;
            }
        }

        var records = new List<Dictionary<string, object>>();
        long sampleElapsed = timer.ElapsedMilliseconds;
        if (stop == null) stop = cancel.IsCancellationRequested ? "cancelled"
            : sampleElapsed >= SampleWindowMs ? "sample_window_limit" : "sample_count_limit";
        var fileTimer = Stopwatch.StartNew();
        string windows = Root(Environment.GetFolderPath(Environment.SpecialFolder.Windows));
        string system = Root(Environment.SystemDirectory);
        long budget = TotalBytes;
        foreach (string path in IdentityOrder(paths, windows, system)) {
            var record = new Dictionary<string, object> { ["name"] = Path.GetFileName(path) };
            records.Add(record);
            bool inWindows = IsUnder(path, windows);
            bool inSystem = IsUnder(path, system);
            record["path"] = inSystem ? "System32/" + path.Substring(system.Length).Replace('\\', '/')
                : inWindows ? "Windows/" + path.Substring(windows.Length).Replace('\\', '/') : path;
            record["identity_status"] = "partial";
            if (!(inSystem || inWindows)) record["identity_reason"] = "outside_windows_root";
            else if (fileTimer.ElapsedMilliseconds >= FileWindowMs || cancel.IsCancellationRequested)
                record["identity_reason"] = "identity_time_limit";
            else {
                try {
                    if ((File.GetAttributes(path) & (FileAttributes.Directory | FileAttributes.ReparsePoint)) != 0) {
                        record["identity_reason"] = "not_regular_file";
                    } else {
                        using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read)) {
                            long bytes = file.Length;
                            record["bytes"] = bytes;
                            if (bytes <= 0 || bytes > FileBytes || bytes > budget) record["identity_reason"] = "identity_byte_limit";
                            else {
                                budget -= bytes;
                                try { record["sha256"] = Convert.ToHexString(SHA256.HashData(file)).ToLowerInvariant(); }
                                catch (Exception error) { record["hash_error"] = error.GetType().Name; }
                                if (fileTimer.ElapsedMilliseconds < FileWindowMs && !cancel.IsCancellationRequested) {
                                    try {
                                        var version = FileVersionInfo.GetVersionInfo(path);
                                        if (!String.IsNullOrEmpty(version.FileVersion)) {
                                            record["file_version"] = $"{version.FileMajorPart}.{version.FileMinorPart}.{version.FileBuildPart}.{version.FilePrivatePart}";
                                        }
                                    } catch (Exception error) { record["version_error"] = error.GetType().Name; }
                                }
                                if (record.ContainsKey("sha256") && record.ContainsKey("file_version"))
                                    record["identity_status"] = "observed";
                                else record["identity_reason"] = "hash_or_version_unavailable";
                            }
                        }
                    }
                } catch (Exception error) { record["identity_reason"] = error.GetType().Name; }
            }
            if ((string)record["identity_status"] != "observed") incomplete++;
        }
        string status = records.Count == 0 ? "unavailable" : errors > 0 || incomplete > 0 ? "partial" : "observed";
        return new Dictionary<string, object> {
            ["status"] = status, ["reason"] = records.Count == 0 ?
                (nonempty == 0 ? "no_nonempty_module_sample" : "no_selected_dll_seen") : "selected_module_samples",
            ["sample_stop"] = stop, ["sample_elapsed_ms"] = sampleElapsed,
            ["identity_elapsed_ms"] = fileTimer.ElapsedMilliseconds,
            ["samples_attempted"] = attempts, ["samples_nonempty"] = nonempty, ["native_errors"] = errors,
            ["last_win32_error"] = lastError, ["incomplete_observations"] = incomplete, ["modules"] = records
        };
    }

    private static IEnumerable<string> IdentityOrder(SortedSet<string> paths, string windows, string system)
    {
        // Stable sort keeps the existing path order within each priority.
        return paths.OrderBy(path => {
            if (!IsUnder(path, windows) && !IsUnder(path, system)) return 3;
            string name = Path.GetFileName(path);
            if (name.Equals("vcruntime140.dll", StringComparison.OrdinalIgnoreCase)) return 0;
            if (name.Equals("ucrtbase.dll", StringComparison.OrdinalIgnoreCase)) return 1;
            return SelectedName.Match(name).Groups["crt"].Success ? 2 : 3;
        });
    }

    private static bool IsUnder(string path, string root) =>
        root.Length != 0 && path.StartsWith(root, StringComparison.OrdinalIgnoreCase);

    private static string Root(string path) => path.Length == 0 ? path : Path.TrimEndingDirectorySeparator(path) + "\\";
}
'@
        }
        $State.loaded_dlls['limits'] = [CueglintGuiDllProbe]::Limits()
        return $true
    } catch {
        $State.loaded_dlls.status = 'failed'
        $State.loaded_dlls.reason = 'observer_setup_failed'
        $State.loaded_dlls['error_type'] = $_.Exception.GetBaseException().GetType().Name
        return $false
    }
}

function Complete-GuiDllProbe($Task, $Cancel, [Collections.IDictionary]$State) {
    if ($null -eq $Task) { return }
    try {
        if ($Task.Wait([int]$State.loaded_dlls.result_wait_ms)) {
            $result = $Task.GetAwaiter().GetResult()
            foreach ($key in $result.Keys) { $State.loaded_dlls[$key] = $result[$key] }
        } else {
            $State.loaded_dlls.reason = 'observer_result_deadline'
            $Cancel.Cancel()
        }
    } catch {
        $State.loaded_dlls.status = 'failed'
        $State.loaded_dlls.reason = 'observer_task_failed'
        $State.loaded_dlls['error_type'] = $_.Exception.GetBaseException().GetType().Name
    } finally { $Cancel.Dispose() }
}

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
    $canProbeDlls = Initialize-GuiDllProbe $State
    $dllTask = $null; $dllCancel = $null
    $process = $null; $streams = @(); $copies = @(); $failure = $null; $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        foreach ($name in @('stdout','stderr')) {
            $streams += [IO.File]::Open((Join-Path $Out "gui-fixture.$name.txt"), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        }
        $process = [Diagnostics.Process]::Start($start)
        $State['started'] = $true; $State['pid'] = $process.Id
        $copies = @($process.StandardOutput.BaseStream.CopyToAsync($streams[0]), $process.StandardError.BaseStream.CopyToAsync($streams[1]))
        if ($canProbeDlls) {
            try {
                $dllCancel = [Threading.CancellationTokenSource]::new()
                $dllTask = [CueglintGuiDllProbe]::Start($process.SafeHandle, $dllCancel.Token)
                $State.loaded_dlls.reason = 'no_observer_result'
            } catch {
                if ($null -ne $dllCancel) { $dllCancel.Dispose() }
                $State.loaded_dlls.status = 'failed'
                $State.loaded_dlls.reason = 'observer_start_failed'
                $State.loaded_dlls['error_type'] = $_.Exception.GetBaseException().GetType().Name
            }
        }
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
    if ($failure) {
        $State['failure'] = $failure.Exception.Message
        Complete-GuiDllProbe $dllTask $dllCancel $State
        throw $failure
    }
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
    } catch { $State['failure'] = $_.Exception.Message; throw } finally {
        $State['total_elapsed_ms'] = $timer.ElapsedMilliseconds
        Complete-GuiDllProbe $dllTask $dllCancel $State
    }
}
