#requires -Version 7.0
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [int]$ExistingProcessId = 0,
    [string[]]$ApplicationArguments = @(),
    [int]$Seconds = 60,
    [int]$WarmupSeconds = 10,
    [switch]$CollectorMode,
    [ValidateSet('economy','balanced','detailed')][string]$Profile = 'balanced',
    [string]$Output = 'artifacts/performance.json'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'process-io-counters.ps1')
if ($Seconds -lt 10 -or $Seconds -gt 3600) { throw 'Seconds must be 10..3600' }
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$outputPath = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Output))
New-Item -ItemType Directory -Force -Path (Split-Path $outputPath) | Out-Null
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $resolvedExecutable
$startInfo.WorkingDirectory = Split-Path $resolvedExecutable
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
foreach ($argument in $ApplicationArguments) { $startInfo.ArgumentList.Add($argument) }
if ($CollectorMode) { $startInfo.RedirectStandardInput = $true; $startInfo.RedirectStandardOutput = $true; $startInfo.RedirectStandardError = $true }
$ownsProcess = $ExistingProcessId -le 0
if (-not $ownsProcess -and $CollectorMode) { throw 'CollectorMode requires ownership of stdin/stdout' }
$application = if ($ownsProcess) { [Diagnostics.Process]::new() } else { [Diagnostics.Process]::GetProcessById($ExistingProcessId) }
if ($ownsProcess) { $application.StartInfo = $startInfo }
$samples = [Collections.Generic.List[object]]::new()
$knownIds = [Collections.Generic.HashSet[int]]::new()
$seenCpu = @{}
$knownCreation = @{}
$seenIo = @{}
$collectorSnapshots = 0
$latestSnapshot = $null
$readerTask = $null
$logicalProcessors = [Environment]::ProcessorCount
$startedAt = [DateTimeOffset]::UtcNow
try {
    if ($ownsProcess -and -not $application.Start()) { throw 'Application did not start' }
    $knownIds.Add($application.Id) | Out-Null
    if ($CollectorMode) {
        $application.StandardInput.WriteLine('{"command":"profile","profile":"' + $Profile + '"}')
        $application.StandardInput.Flush()
        $readerTask = $application.StandardOutput.ReadLineAsync()
    }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $lastSampleSeconds = 0.0
    while ($watch.Elapsed.TotalSeconds -lt ($WarmupSeconds + $Seconds)) {
        Start-Sleep -Milliseconds 1000
        $application.Refresh()
        if ($application.HasExited) { throw "Application exited early: $($application.ExitCode)" }
        if ($CollectorMode) {
            while ($readerTask.IsCompleted) {
                $line = $readerTask.GetAwaiter().GetResult()
                if ($null -eq $line) { break }
                try { $decoded = $line | ConvertFrom-Json; if ($decoded.ready) { $collectorSnapshots++; $latestSnapshot = $decoded } } catch { }
                $readerTask = $application.StandardOutput.ReadLineAsync()
            }
        }
        # One shared CIM inventory per loop, never a query per PID. Process I/O
        # counts include pipes/network/devices and are NOT physical-disk I/O.
        $inventory = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate,ReadOperationCount,WriteOperationCount,ReadTransferCount,WriteTransferCount)
        $inventoryById = @{}
        foreach ($entry in $inventory) { $inventoryById[[int]$entry.ProcessId] = $entry }
        if (-not $CollectorMode) {
            # Prune exited/reused descendant IDs before discovering current
            # descendants. A reused PID must not charge an unrelated process.
            foreach ($applicationId in @($knownIds)) {
                if ($applicationId -eq $application.Id) { continue }
                $entry = $inventoryById[$applicationId]
                $creation = if ($entry) { Get-NexusProcessCreationKey $entry.CreationDate } else { $null }
                if ($null -eq $entry -or $knownCreation[$applicationId] -ne $creation) {
                    $knownIds.Remove($applicationId) | Out-Null
                    $knownCreation.Remove($applicationId); $seenCpu.Remove($applicationId); $seenIo.Remove($applicationId)
                }
            }
            # Inventory descendants, including WebView subprocesses. Samples
            # exclude the measurement helper's own CPU, memory and process I/O.
            do {
                $added = $false
                foreach ($entry in $inventory) {
                    $parentId = [int]$entry.ParentProcessId
                    if (-not $knownIds.Contains($parentId) -or $knownIds.Contains([int]$entry.ProcessId)) { continue }
                    $creation = Get-NexusProcessCreationKey $entry.CreationDate
                    $parent = $inventoryById[$parentId]
                    $parentCreation = if ($parent) { Get-NexusProcessCreationKey $parent.CreationDate } else { $null }
                    if ($null -ne $creation -and $null -ne $parentCreation -and $creation -ge $parentCreation -and $knownIds.Add([int]$entry.ProcessId)) {
                        $knownCreation[[int]$entry.ProcessId] = $creation
                        $added = $true
                    }
                }
            } while ($added)
        }
        $ioDelta = Get-NexusProcessIoDelta -Inventory $inventory -ProcessIds $knownIds -State $seenIo
        $cpuDeltaMs = 0.0; $workingSet = 0L; $privateBytes = 0L; $processCount = 0
        foreach ($applicationId in @($knownIds)) {
            $tracked = Get-Process -Id $applicationId -ErrorAction SilentlyContinue
            if ($null -eq $tracked) { continue }
            try {
                $cpuMs = $tracked.TotalProcessorTime.TotalMilliseconds
                if ($seenCpu.ContainsKey($applicationId)) { $cpuDeltaMs += [Math]::Max(0, $cpuMs - $seenCpu[$applicationId]) }
                $seenCpu[$applicationId] = $cpuMs
                $workingSet += $tracked.WorkingSet64; $privateBytes += $tracked.PrivateMemorySize64; $processCount++
            } finally { $tracked.Dispose() }
        }
        $elapsedSeconds = $watch.Elapsed.TotalSeconds
        $sampleSeconds = $elapsedSeconds - $lastSampleSeconds
        $lastSampleSeconds = $elapsedSeconds
        if ($elapsedSeconds -ge $WarmupSeconds) {
            $samples.Add([pscustomobject]@{elapsedSeconds=[Math]::Round($elapsedSeconds,3);sampleSeconds=[Math]::Round($sampleSeconds,6);cpuPercent=[Math]::Round($cpuDeltaMs/($sampleSeconds*1000*$logicalProcessors)*100,4);workingSetMiB=[Math]::Round($workingSet/1MB,3);privateMiB=[Math]::Round($privateBytes/1MB,3);processes=$processCount;processIo=$ioDelta})
        }
    }
    $ioSamples = @($samples | Where-Object { $_.processIo.deltaProcesses -gt 0 })
    $ioCompleteSamples = @($samples | Where-Object { $_.processIo.status -eq 'complete' }).Count
    $ioSeconds = [double](($ioSamples | Measure-Object sampleSeconds -Sum).Sum)
    $observedIo = [ordered]@{}
    foreach ($field in @('readOperations','writeOperations','readBytes','writeBytes')) {
        # Sum exact decimal deltas; Measure-Object would convert them to double.
        $total = [decimal]0
        foreach ($sample in $ioSamples) { $total += $sample.processIo.$field }
        $observedIo[$field] = if ($ioSamples.Count) { $total } else { $null }
    }
    $result = [ordered]@{
        recordedAt=$startedAt.ToString('o'); executable=$resolvedExecutable; sha256=(Get-FileHash -LiteralPath $resolvedExecutable -Algorithm SHA256).Hash
        attachedToExistingProcess=(-not $ownsProcess)
        mode=$(if ($CollectorMode) {'collector-only'} else {'application-process-tree'}); profile=$Profile; logicalProcessors=$logicalProcessors
        warmupSeconds=$WarmupSeconds; requestedMeasurementSeconds=$Seconds; sampleCount=$samples.Count
        averageCpuPercent=($samples | Measure-Object cpuPercent -Average).Average
        maximumCpuPercent=($samples | Measure-Object cpuPercent -Maximum).Maximum
        averageWorkingSetMiB=($samples | Measure-Object workingSetMiB -Average).Average
        maximumWorkingSetMiB=($samples | Measure-Object workingSetMiB -Maximum).Maximum
        averagePrivateMiB=($samples | Measure-Object privateMiB -Average).Average
        maximumProcesses=($samples | Measure-Object processes -Maximum).Maximum
        physicalDiskIoMeasured=$false; gpuActivityMeasured=$false
        processIo=[ordered]@{
            source='Win32_Process ReadOperationCount/WriteOperationCount/ReadTransferCount/WriteTransferCount'
            scope='Observed process I/O: files, IPC/pipes, network and devices; not physical-disk I/O'
            measured=($ioSamples.Count -gt 0)
            status=$(if (-not $ioSamples.Count) {'unavailable'} elseif ($ioCompleteSamples -eq $samples.Count) {'complete'} else {'partial'})
            samplesWithDeltas=$ioSamples.Count; completeSamples=$ioCompleteSamples
            partialSamples=@($samples | Where-Object { $_.processIo.status -eq 'partial' }).Count
            baselineSamples=@($samples | Where-Object { $_.processIo.status -eq 'baseline' }).Count
            unavailableSamples=@($samples | Where-Object { $_.processIo.status -eq 'unavailable' }).Count
            observedIntervalSeconds=[Math]::Round($ioSeconds,6)
            observedReadOperations=$observedIo.readOperations; observedWriteOperations=$observedIo.writeOperations
            observedReadBytes=$observedIo.readBytes; observedWriteBytes=$observedIo.writeBytes
            averageObservedReadOperationsPerSecond=$(if ($ioSeconds -gt 0) { [Math]::Round([double]$observedIo.readOperations/$ioSeconds,3) } else { $null })
            averageObservedWriteOperationsPerSecond=$(if ($ioSeconds -gt 0) { [Math]::Round([double]$observedIo.writeOperations/$ioSeconds,3) } else { $null })
            averageObservedReadBytesPerSecond=$(if ($ioSeconds -gt 0) { [Math]::Round([double]$observedIo.readBytes/$ioSeconds,3) } else { $null })
            averageObservedWriteBytesPerSecond=$(if ($ioSeconds -gt 0) { [Math]::Round([double]$observedIo.writeBytes/$ioSeconds,3) } else { $null })
        }
        readySnapshots=$collectorSnapshots
        sensorAvailability=$(if ($latestSnapshot) { @($latestSnapshot.components | ForEach-Object { [pscustomobject]@{kind=$_.kind;metrics=@($_.metrics | ForEach-Object { [pscustomobject]@{id=$_.id;available=($null -ne $_.value);source=$_.source} })} }) } else { @() })
        limitations=@('CPU is normalized to all logical processors, as in Task Manager.','Working set sums can include shared pages counted by more than one process.','Process I/O includes file, pipe/IPC, network and device operations; it is not a physical-disk measurement.','Initial/unavailable/reused-PID observations establish a new baseline. Short-lived processes and I/O after their last inventory may be missed; observed totals are not lifetime totals.','One shared CIM inventory runs per loop; measurement helper overhead is excluded from the application tree.','Physical disk I/O, GPU activity, reboot/sleep and game FPS are not measured by this script.','Results apply only to this machine and measurement window; this is not a controlled game benchmark.')
        samples=$samples.ToArray()
    }
    $result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $outputPath -Encoding utf8
    [pscustomobject]$result | Select-Object mode,profile,sampleCount,averageCpuPercent,maximumCpuPercent,averageWorkingSetMiB,maximumWorkingSetMiB,maximumProcesses,readySnapshots | Format-List
} finally {
    if ($ownsProcess -and $application.Id -and -not $application.HasExited) {
        if ($CollectorMode) { $application.StandardInput.Close(); $application.WaitForExit(5000) | Out-Null }
        if (-not $application.HasExited) { $application.Kill($true); $application.WaitForExit() }
    }
    $application.Dispose()
}
