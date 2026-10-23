#requires -Version 7.0
# Deterministic fixtures only: no processes are started and no CIM poll runs.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'process-io-counters.ps1')
$script:nexusIoAssertions = 0
function Assert-NexusIo($Actual, $Expected, [string]$Message) {
    $script:nexusIoAssertions++
    if ($Actual -cne $Expected) { throw "$Message (actual=$Actual, expected=$Expected)" }
}
function New-NexusIoFixture([int]$Id, [object]$Read, [object]$Write, [object]$ReadBytes, [object]$WriteBytes, [string]$Born='2026-10-20T00:00:00Z') {
    [pscustomobject]@{ProcessId=$Id; CreationDate=[DateTimeOffset]::Parse($Born); ReadOperationCount=$Read; WriteOperationCount=$Write; ReadTransferCount=$ReadBytes; WriteTransferCount=$WriteBytes}
}
$selected = [Collections.Generic.HashSet[int]]::new([int[]]@(10,11))
$state = @{}
$first = @(New-NexusIoFixture 10 100 200 1000 2000; New-NexusIoFixture 11 3 4 500 600; New-NexusIoFixture 99 999 999 999 999)
$baseline = Get-NexusProcessIoDelta $first $selected $state
Assert-NexusIo $baseline.status 'baseline' 'First observation must not count process lifetime I/O'
Assert-NexusIo $baseline.readBytes $null 'Unknown initial delta must remain null'
Assert-NexusIo $baseline.trackedProcesses 2 'Unrelated processes must not enter the tree sum'
$next = @(New-NexusIoFixture 10 105 207 1100 2400; New-NexusIoFixture 11 6 8 520 650)
$delta = Get-NexusProcessIoDelta $next $selected $state
Assert-NexusIo $delta.status 'complete' 'Two valid intervals'
Assert-NexusIo $delta.readOperations ([decimal]8) 'Read operations must sum per-process deltas'
Assert-NexusIo $delta.writeOperations ([decimal]11) 'Write operations must sum per-process deltas'
Assert-NexusIo $delta.readBytes ([decimal]120) 'Bytes are not operation counts'
Assert-NexusIo $delta.writeBytes ([decimal]450) 'Write bytes must sum separately'
$zero = Get-NexusProcessIoDelta $next $selected $state
Assert-NexusIo $zero.readOperations ([decimal]0) 'A measured zero must stay numeric'
Assert-NexusIo $zero.status 'complete' 'Zero is a valid measured interval'
$partial = Get-NexusProcessIoDelta @((New-NexusIoFixture 10 106 208 1110 2410), (New-NexusIoFixture 11 $null 10 530 660)) $selected $state
Assert-NexusIo $partial.status 'partial' 'Missing counters must be flagged'
Assert-NexusIo $partial.readBytes ([decimal]10) 'Partial samples sum only observable intervals'
Assert-NexusIo $partial.unavailableProcesses 1 'Unavailable process must be counted'
$recovered = Get-NexusProcessIoDelta @((New-NexusIoFixture 11 10 10 600 700)) $selected $state
Assert-NexusIo $recovered.status 'baseline' 'Recovery must not bridge the missing interval'
Assert-NexusIo $recovered.readBytes $null 'Recovered lifetime is not one-second I/O'
$reused = Get-NexusProcessIoDelta @((New-NexusIoFixture 11 9000 9000 9000 9000 '2026-10-20T00:00:05Z')) $selected $state
Assert-NexusIo $reused.status 'baseline' 'Reused PID must get a new baseline'
Assert-NexusIo $reused.writeBytes $null 'PID reuse must not create a large false delta'
$reset = Get-NexusProcessIoDelta @((New-NexusIoFixture 11 1 1 1 1 '2026-10-20T00:00:05Z')) $selected $state
Assert-NexusIo $reset.counterResetProcesses 1 'Counter rollback must be reported'
Assert-NexusIo $reset.readBytes $null 'Counter rollback must not become negative traffic'
$resumed = Get-NexusProcessIoDelta @((New-NexusIoFixture 11 3 4 10 20 '2026-10-20T00:00:05Z')) $selected $state
Assert-NexusIo $resumed.writeBytes ([decimal]19) 'Counters must resume after reset'
$largeState=@{}
$largeBase = Get-NexusProcessIoDelta @((New-NexusIoFixture 10 '18446744073709551500' 0 '18446744073709551500' 0)) $selected $largeState
$largeDelta = Get-NexusProcessIoDelta @((New-NexusIoFixture 10 '18446744073709551507' 0 '18446744073709551511' 0)) $selected $largeState
Assert-NexusIo $largeDelta.readOperations ([decimal]7) 'uint64 deltas must not lose precision through double'
Assert-NexusIo $largeDelta.readBytes ([decimal]11) 'Large uint64 byte counter precision'
$bad = Get-NexusProcessIoDelta @((New-NexusIoFixture 10 'not-a-number' 0 0 0)) $selected $largeState
Assert-NexusIo $bad.status 'unavailable' 'Invalid counter is unavailable, never zero'
Assert-NexusIo $bad.readOperations $null 'Invalid values must not fabricate zero operations'
$gone = Get-NexusProcessIoDelta @() $selected $state
Assert-NexusIo $gone.status 'unavailable' 'No observed process must be unavailable'
Assert-NexusIo $state.Count 0 'Exited process state must be released'
$missingBirth = New-NexusIoFixture 10 0 0 0 0
$missingBirth.CreationDate = $null
$unknownBirth = Get-NexusProcessIoDelta @($missingBirth) $selected @{}
Assert-NexusIo $unknownBirth.status 'unavailable' 'Unknown birth identity cannot identify reused PIDs'
$omittedField = New-NexusIoFixture 10 0 0 0 0
$omittedField.PSObject.Properties.Remove('WriteTransferCount')
$missingCounter = Get-NexusProcessIoDelta @($omittedField) $selected @{}
Assert-NexusIo $missingCounter.status 'unavailable' 'Missing provider property must remain unavailable under strict mode'
$missingBirth.PSObject.Properties.Remove('CreationDate')
$missingCreation = Get-NexusProcessIoDelta @($missingBirth) $selected @{}
Assert-NexusIo $missingCreation.readBytes $null 'Missing provider birth property must not fabricate data'
[pscustomobject]@{check='process-io-counters';result='pass';assertions=$script:nexusIoAssertions;mode='synthetic fixtures; no hardware or process workload'} | ConvertTo-Json
