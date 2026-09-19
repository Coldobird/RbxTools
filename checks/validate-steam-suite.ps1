param(
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [switch]$Simulation,
    [switch]$Reblock,
    [string]$FixedCandidateOrder
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$probe = Join-Path $PSScriptRoot "steam-connection-probe.ps1"
$testTarget = Join-Path $root "src-tauri\target\debug\examples\wfp_test_target.exe"
$monitorDuration = if ($Reblock) { "15" } else { "12" }

$probeArguments = @(
    "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ('"' + $probe + '"'),
    "-Mode", "Experiment", "-DurationSeconds", $monitorDuration, "-IntervalMilliseconds", "250",
    "-LongBlockSeconds", "2", "-GraceSeconds", "1", "-CandidateObservationSeconds", "1",
    "-OutputPath", ('"' + $OutputPath + '"')
)
if ($Simulation) {
    $probeArguments += @(
        "-Simulation", "-SimulatedRecoveryAfterCandidates", "2",
        "-CandidateStatsPath", ('"' + $OutputPath + '.stats.json' + '"')
    )
}
if ($FixedCandidateOrder) {
    $probeArguments += @("-FixedCandidateOrder", ('"' + $FixedCandidateOrder + '"'))
}
$monitor = Start-Process powershell.exe -WindowStyle Hidden -ArgumentList $probeArguments -PassThru
Start-Sleep -Seconds 4
$pulse = Start-Process $testTarget -WindowStyle Hidden -ArgumentList "3" -PassThru -Wait
if ($pulse.ExitCode -ne 0) { throw "WFP test target exited with code $($pulse.ExitCode)." }
if ($Reblock) {
    Start-Sleep -Milliseconds 1500
    $secondPulse = Start-Process $testTarget -WindowStyle Hidden -ArgumentList "2" -PassThru -Wait
    if ($secondPulse.ExitCode -ne 0) { throw "Second WFP test target exited with code $($secondPulse.ExitCode)." }
}
$monitor.WaitForExit()
if ($monitor.ExitCode -ne 0) { throw "Probe exited with code $($monitor.ExitCode)." }
