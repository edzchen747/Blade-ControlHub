<#
.SYNOPSIS
    Runs the ambient detection benchmark and samples the OS performance
    counters for it while it runs.

.DESCRIPTION
    src/bin/ambient_bench.rs measures CPU and memory from inside the process,
    which is the accurate way to attribute those to the capture thread. It
    cannot see GPU usage, so this wrapper samples the same PDH counters that
    Task Manager and Resource Monitor read:

        \GPU Engine(pid_<pid>_*)\Utilization Percentage

    broken down per engine type, plus an independent check of process CPU time
    and working set.

.EXAMPLE
    pwsh -File scripts/ambient-bench.ps1 -Seconds 30

.NOTES
    Counter paths are English. On a localised Windows install, Get-Counter
    needs the translated names and this wrapper will report the GPU section as
    unavailable; the benchmark's own CPU and memory numbers are unaffected.
#>
[CmdletBinding()]
param(
    [int]$Seconds = 30,
    [int]$Warmup = 3,
    [switch]$Uncapped,
    [switch]$NormalPriority,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repoRoot 'target/release/ambient_bench.exe'

if (-not $SkipBuild) {
    Write-Host 'Building ambient_bench (release)...' -ForegroundColor Cyan
    & cargo build --release --bin ambient_bench --manifest-path (Join-Path $repoRoot 'cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path $exe)) { throw "benchmark binary not found at $exe" }

$benchArgs = @('--seconds', $Seconds, '--warmup', $Warmup)
if ($Uncapped) { $benchArgs += '--uncapped' }
if ($NormalPriority) { $benchArgs += '--normal-priority' }

$stdout = New-TemporaryFile
$stderr = New-TemporaryFile

Write-Host "Running detection for $Seconds s (plus $Warmup s warmup)..." -ForegroundColor Cyan
$proc = Start-Process -FilePath $exe -ArgumentList $benchArgs -PassThru -NoNewWindow `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr

# ── Sample the OS counters for the live process ─────────────────────────────
$gpuPath = "\GPU Engine(pid_$($proc.Id)_*)\Utilization Percentage"
$gpuTotals = [System.Collections.Generic.List[double]]::new()
$gpuByEngine = @{}
$workingSets = [System.Collections.Generic.List[double]]::new()
$gpuCounterError = $null

while (-not $proc.HasExited) {
    try {
        $samples = (Get-Counter -Counter $gpuPath -ErrorAction Stop).CounterSamples
        $total = 0.0
        foreach ($sample in $samples) {
            $total += $sample.CookedValue
            # Instance looks like pid_123_luid_..._phys_0_eng_0_engtype_3D
            if ($sample.InstanceName -match 'engtype_(?<engine>\w+)$') {
                $engine = $Matches.engine
                if (-not $gpuByEngine.ContainsKey($engine)) {
                    $gpuByEngine[$engine] = [System.Collections.Generic.List[double]]::new()
                }
                $gpuByEngine[$engine].Add($sample.CookedValue)
            }
        }
        $gpuTotals.Add($total)
    }
    catch {
        # No GPU engine instances yet, or the counter set is unavailable.
        if (-not $gpuCounterError) { $gpuCounterError = $_.Exception.Message }
    }

    $live = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
    if ($live) { $workingSets.Add($live.WorkingSet64 / 1MB) }
}

$proc.WaitForExit()

Get-Content $stdout | Write-Host
$errorText = (Get-Content $stderr -Raw)
if ($errorText -and $errorText.Trim()) { Write-Host $errorText.Trim() -ForegroundColor Red }
Remove-Item $stdout, $stderr -Force -ErrorAction SilentlyContinue

# ── Report what the OS saw ──────────────────────────────────────────────────
Write-Host 'GPU (PDH counters, the source Task Manager and resmon read)'
if ($gpuTotals.Count -gt 0) {
    $avg = ($gpuTotals | Measure-Object -Average).Average
    $max = ($gpuTotals | Measure-Object -Maximum).Maximum
    Write-Host ('  samples          {0,9}' -f $gpuTotals.Count)
    Write-Host ('  mean total       {0,9:N3} %' -f $avg)
    Write-Host ('  peak total       {0,9:N3} %' -f $max)
    foreach ($engine in ($gpuByEngine.Keys | Sort-Object)) {
        $engineAvg = ($gpuByEngine[$engine] | Measure-Object -Average).Average
        Write-Host ('  engtype {0,-16}{1,9:N3} % mean' -f $engine, $engineAvg)
    }
}
else {
    Write-Host '  unavailable' -ForegroundColor Yellow
    if ($gpuCounterError) { Write-Host "  reason: $gpuCounterError" -ForegroundColor Yellow }
    Write-Host '  (the process may have used too little GPU time to register an engine instance)'
}
Write-Host ''

Write-Host 'Process working set (sampled externally, cross-check)'
if ($workingSets.Count -gt 0) {
    Write-Host ('  mean             {0,9:N2} MB' -f ($workingSets | Measure-Object -Average).Average)
    Write-Host ('  peak             {0,9:N2} MB' -f ($workingSets | Measure-Object -Maximum).Maximum)
}
else {
    Write-Host '  unavailable' -ForegroundColor Yellow
}
Write-Host ''

if ($proc.ExitCode -ne 0) {
    Write-Host "ambient_bench exited with code $($proc.ExitCode)" -ForegroundColor Red
    exit $proc.ExitCode
}
