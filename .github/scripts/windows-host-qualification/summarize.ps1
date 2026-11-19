# Only known, generated text evidence is printed. Binaries and original license
# files are never read by this summarizer or uploaded by the workflow.
param([Parameter(Mandatory)][string]$Out)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
if (-not $Out.StartsWith([IO.Path]::GetFullPath($env:RUNNER_TEMP) + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Evidence must be inside RUNNER_TEMP.'
}
Write-Output "Qualification run $env:GITHUB_RUN_ID attempt $env:GITHUB_RUN_ATTEMPT controller $env:GITHUB_SHA"
Write-Output 'Non-distributing technical probe; Windows notices are not adopted and distribution readiness remains false.'
if (-not (Test-Path -LiteralPath $Out -PathType Container)) {
    Write-Output 'No qualification evidence directory: inspect preceding setup/parse step failures.'
    exit 0
}
$files = @(Get-ChildItem -LiteralPath $Out -File | Sort-Object Name)
if ($files.Count -gt 64) { throw 'Unexpected evidence file count; no broad enumeration.' }
$hashedBytes = 0L
foreach ($file in $files) {
    if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected evidence reparse point.' }
    if ($file.Extension -notin @('.txt', '.json', '.jsonl', '.cmd')) { throw 'Unexpected evidence file type.' }
    if ($file.Length -gt 32MB -or $hashedBytes + $file.Length -gt 64MB) {
        Write-Output "Evidence $($file.Name): $($file.Length) bytes; hash/read skipped at the explicit evidence bound."
        continue
    }
    $hashedBytes += $file.Length
    $digest = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output "Evidence $($file.Name): $($file.Length) bytes sha256=$digest"
}
$text = @('qualification.json', 'controller-head.stdout.txt', 'source-head.stdout.txt', 'source-tree.stdout.txt',
    'rustc-version.stdout.txt', 'cargo-version.stdout.txt', 'selected-tool-vars.txt', 'graph-diff.json',
    'gui-pe.txt', 'cli-pe.txt', 'cli-help.stdout.txt', 'cli-help.stderr.txt', 'gui-fixture.stdout.txt', 'gui-fixture.stderr.txt')
foreach ($name in $text) {
    $path = Join-Path $Out $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
    if ((Get-Item -LiteralPath $path).Length -gt 64KB) {
        Write-Output "$name omitted from log body at 64 KiB; its length/hash were reported above."
        continue
    }
    Write-Output "--- $name ---"
    # Echo with a prefix so generated text cannot become a GitHub workflow command.
    Get-Content -LiteralPath $path | ForEach-Object { Write-Output "| $_" }
}
$tree = Join-Path $Out 'tree.stdout.txt'
if ((Test-Path -LiteralPath $tree -PathType Leaf) -and (Get-Item -LiteralPath $tree).Length -le 512KB) {
    Write-Output '--- native locked tree ---'
    Get-Content -LiteralPath $tree | ForEach-Object { Write-Output "| $_" }
}
$failures = @($files | Where-Object { $_.Name.EndsWith('.stderr.txt', [StringComparison]::Ordinal) -and $_.Length })
foreach ($file in $failures) {
    if ($file.Length -gt 32MB) {
        Write-Output "$($file.Name) excerpts skipped at the explicit 32 MiB per-file read bound."
        continue
    }
    Write-Output "--- $($file.Name): first 40 and last 80 lines; middle may be omitted ---"
    Get-Content -LiteralPath $file.FullName -TotalCount 40 | ForEach-Object { Write-Output ('| ' + $_.Substring(0, [Math]::Min($_.Length, 2048))) }
    Get-Content -LiteralPath $file.FullName -Tail 80 | ForEach-Object { Write-Output ('| ' + $_.Substring(0, [Math]::Min($_.Length, 2048))) }
}
Write-Output 'Always-step completion does not imply qualification success. No binary or vendor license artifact was uploaded.'
