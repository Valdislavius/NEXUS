# Pure helpers for the resource sampler; loading this file performs no polling.
# Win32_Process Read/WriteOperationCount are cumulative operation counts and
# Read/WriteTransferCount are cumulative bytes, not physical-disk counters.
# https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-process
# Process I/O includes files, network and devices; ReadFile also accepts pipes.
# https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-wsrm/c0a51f88-04b5-4945-ba86-e8e8cf2df1fe
# https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-readfile

function Get-NexusProcessCreationKey {
    param([object]$Value)
    if ($null -eq $Value) { return $null }
    try { return ([DateTimeOffset]$Value).UtcTicks } catch { return $null }
}

function Get-NexusProcessIoDelta {
    param(
        [object[]]$Inventory,
        [Collections.Generic.HashSet[int]]$ProcessIds,
        [hashtable]$State
    )
    $fields = @('ReadOperationCount','WriteOperationCount','ReadTransferCount','WriteTransferCount')
    $totals = [decimal[]]@(0,0,0,0)
    $tracked = 0; $deltas = 0; $baselines = 0; $unavailable = 0; $resets = 0
    $liveIds = [Collections.Generic.HashSet[int]]::new()
    foreach ($entry in $Inventory) {
        $applicationId = [int]$entry.ProcessId
        if (-not $ProcessIds.Contains($applicationId) -or -not $liveIds.Add($applicationId)) { continue }
        $tracked++
        $birthProperty = $entry.PSObject.Properties['CreationDate']
        $creation = if ($birthProperty) { Get-NexusProcessCreationKey $birthProperty.Value } else { $null }
        $values = [decimal[]]@(0,0,0,0)
        $valid = $null -ne $creation
        for ($index = 0; $index -lt $fields.Count; $index++) {
            $property = $entry.PSObject.Properties[$fields[$index]]
            $value = if ($property) { $property.Value } else { $null }
            $parsed = [uint64]0
            if ($null -eq $value -or -not [uint64]::TryParse([string]$value, [Globalization.NumberStyles]::None, [Globalization.CultureInfo]::InvariantCulture, [ref]$parsed)) {
                $valid = $false; break
            }
            $values[$index] = [decimal]$parsed
        }
        if (-not $valid) {
            # Do not bridge unavailable intervals and divide them by one second.
            $unavailable++; $State.Remove($applicationId); continue
        }
        $previous = $State[$applicationId]
        $State[$applicationId] = @{creation=$creation;values=$values}
        if ($null -eq $previous -or $previous.creation -ne $creation) { $baselines++; continue }
        $reset = $false
        for ($index = 0; $index -lt $fields.Count; $index++) {
            if ($values[$index] -lt $previous.values[$index]) { $reset = $true; break }
        }
        if ($reset) { $resets++; $baselines++; continue }
        for ($index = 0; $index -lt $fields.Count; $index++) { $totals[$index] += $values[$index] - $previous.values[$index] }
        $deltas++
    }
    foreach ($applicationId in @($State.Keys)) {
        if (-not $liveIds.Contains([int]$applicationId)) { $State.Remove($applicationId) }
    }
    $status = if ($tracked -eq 0 -or $unavailable -eq $tracked) {'unavailable'} elseif ($deltas -eq 0) {'baseline'} elseif ($deltas -eq $tracked) {'complete'} else {'partial'}
    [pscustomobject]@{
        status=$status; trackedProcesses=$tracked; deltaProcesses=$deltas
        baselineProcesses=$baselines; unavailableProcesses=$unavailable; counterResetProcesses=$resets
        readOperations=$(if ($deltas) {$totals[0]} else {$null})
        writeOperations=$(if ($deltas) {$totals[1]} else {$null})
        readBytes=$(if ($deltas) {$totals[2]} else {$null})
        writeBytes=$(if ($deltas) {$totals[3]} else {$null})
    }
}
