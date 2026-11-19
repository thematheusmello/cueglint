# Exercise only the pure trace and PE checks. The Windows probe is not executed.
$ErrorActionPreference = 'Stop'
$tokens = $null
$errors = $null
$script = [System.Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $PSScriptRoot 'link-calibration.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($name in @('Read-LinkCalibrationTrace', 'Test-LinkCalibrationTrace', 'Get-LinkCalibrationOutline',
    'Test-LinkCalibrationMap', 'Get-LinkCalibrationIdentity')) {
    $function = $script.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name
    }, $true)
    . ([ScriptBlock]::Create($function.Extent.Text))
}

function Diagnostic([string]$Trace) {
    @{ code = @{ code = 'linker_messages' }; level = 'warning'; message = "linker stdout: $Trace" } |
        ConvertTo-Json -Depth 3 -Compress
}
$archive = 'C:\Fixture Evidence\cueglint_calibration.lib'
$trace = @"
Searching libraries
    Searching $archive`:
      Found cueglint_calibration_used
        Referenced in original.obj
        Loaded cueglint_calibration.lib(used.obj)
Finished searching libraries
"@
$positive = Diagnostic $trace
if (-not (Test-LinkCalibrationTrace $positive $archive) -or
    -not (Test-LinkCalibrationTrace (Diagnostic ($trace.Replace('Loaded cueglint_calibration.lib', "Loaded $archive"))) $archive)) {
    throw 'The controlled selection was not recognized.'
}
foreach ($negative in @(
    (Diagnostic ($trace.Replace('Loaded cueglint_calibration.lib(used.obj)', ''))),
    (Diagnostic ($trace.Replace('used.obj)', 'unused.obj)'))),
    (Diagnostic ($trace.Replace('Found cueglint_calibration_used', 'Found another_symbol'))),
    (Diagnostic ($trace.Replace($archive, 'C:\Alien\cueglint_calibration.lib'))),
    (Diagnostic ($trace.Replace('Finished searching libraries', ''))),
    (Diagnostic ($trace.Replace('Referenced in original.obj', 'Loaded cueglint_calibration.lib(used.obj)'))),
    ($positive + "`n" + $positive),
    $positive.Replace('linker stdout:', 'linker stderr:')
)) {
    if (Test-LinkCalibrationTrace $negative $archive) { throw 'Ambiguous or incomplete trace passed.' }
}

$outline = @(Get-LinkCalibrationOutline (Read-LinkCalibrationTrace $positive))
if ($outline.Count -ne 3 -or $outline[0].kind -cne 'searching' -or
    -not $outline[1].used_symbol -or -not $outline[2].used_member -or
    (($outline | ConvertTo-Json -Compress) -match 'Fixture Evidence|original.obj|cueglint_calibration')) {
    throw 'The trace outline missed a control or exposed source text.'
}
$map = @'
 cueglint_calibration
 Address         Publics by Value              Rva+Base               Lib:Object
 0001:00000000    unused_named_other            0000000140001000 f     patch.obj
 0001:00000040    cueglint_calibration_used      0000000140001040 f     cueglint_calibration.lib:used.obj
'@
if (-not (Test-LinkCalibrationMap $map $archive) -or
    -not (Test-LinkCalibrationMap ($map.Replace('cueglint_calibration.lib:used.obj', 'cueglint_calibration:used.obj')) $archive) -or
    -not (Test-LinkCalibrationMap ($map.Replace('cueglint_calibration.lib:used.obj', "${archive}:used.obj")) $archive)) {
    throw 'The owned map symbol was not recognized.'
}
foreach ($negative in @(
    $map.Replace('cueglint_calibration.lib:used.obj', 'wrong.lib:used.obj'),
    $map.Replace('f     cueglint_calibration.lib:used.obj', 'f     used.obj'),
    $map.Replace('Publics by Value','Static symbols'),
    $map.Replace(' 0001:00000040', " Static symbols`n 0001:00000040"),
    ($map + "`n 0001:00000060 cueglint_calibration_unused 0000000140001060 f cueglint_calibration:unused.obj"),
    ($map + "`n 0001:00000080 cueglint_calibration_used 0000000140001080 f cueglint_calibration:used.obj")
)) {
    if (Test-LinkCalibrationMap $negative $archive) { throw 'An ambiguous map origin passed.' }
}

$directory = [IO.Directory]::CreateTempSubdirectory('cueglint-link-calibration-test-')
try {
    $exe = Join-Path $directory.FullName 'synthetic.exe'
    $bytes = [byte[]]::new(96)
    $bytes[0] = 0x4d
    $bytes[1] = 0x5a
    $bytes[0x3c] = 64
    $bytes[64] = 0x50
    $bytes[65] = 0x45
    $bytes[68] = 0x64
    $bytes[69] = 0x86
    $bytes[88] = 0x0b
    $bytes[89] = 0x02
    [IO.File]::WriteAllBytes($exe, $bytes)
    $identity = Get-LinkCalibrationIdentity $exe -Amd64Pe
    if ($identity.bytes -ne $bytes.Length -or $identity.sha256 -ine (Get-FileHash -LiteralPath $exe).Hash) {
        throw 'The PE digest did not describe the bytes that were checked.'
    }
    $bytes[69] = 0
    [IO.File]::WriteAllBytes($exe, $bytes)
    $refused = $false
    try { Get-LinkCalibrationIdentity $exe -Amd64Pe | Out-Null } catch { $refused = $true }
    if (-not $refused) { throw 'The PE check accepted a different machine.' }
} finally { $directory.Delete($true) }
Write-Output 'Windows linker controls passed: trace selection and redaction, map origin refusal cases and AMD64 PE identity.'
