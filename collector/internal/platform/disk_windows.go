//go:build windows

package platform

import (
	"fmt"
	"golang.org/x/sys/windows"
	"unsafe"
)

type physicalCounters struct {
	read, write                       uint64
	readTime, writeTime, idleTime     uint64
	readCount, writeCount, queueDepth uint32
	queryTime                         int64
}

// Layout of Windows DISK_PERFORMANCE (ntdddisk.h), including native alignment.
type diskPerformance struct {
	BytesRead, BytesWritten, ReadTime, WriteTime, IdleTime int64
	ReadCount, WriteCount, QueueDepth, SplitCount          uint32
	QueryTime                                              int64
	StorageDeviceNumber                                    uint32
	StorageManagerName                                     [8]uint16
	Padding                                                uint32
}

func readPhysicalCounters(index uint32) (physicalCounters, bool) {
	path, err := windows.UTF16PtrFromString(fmt.Sprintf(`\\.\PhysicalDrive%d`, index))
	if err != nil {
		return physicalCounters{}, false
	}
	handle, err := windows.CreateFile(path, 0, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, nil, windows.OPEN_EXISTING, 0, 0)
	if err != nil {
		return physicalCounters{}, false
	}
	defer windows.CloseHandle(handle)
	var perf diskPerformance
	var returned uint32
	err = windows.DeviceIoControl(handle, 0x70020, nil, 0, (*byte)(unsafe.Pointer(&perf)), uint32(unsafe.Sizeof(perf)), &returned, nil)
	if err != nil || returned < uint32(unsafe.Sizeof(perf)) || perf.BytesRead < 0 || perf.BytesWritten < 0 || perf.ReadTime < 0 || perf.WriteTime < 0 || perf.IdleTime < 0 || perf.QueryTime <= 0 {
		return physicalCounters{}, false
	}
	return physicalCounters{read: uint64(perf.BytesRead), write: uint64(perf.BytesWritten), readTime: uint64(perf.ReadTime), writeTime: uint64(perf.WriteTime), idleTime: uint64(perf.IdleTime), readCount: perf.ReadCount, writeCount: perf.WriteCount, queueDepth: perf.QueueDepth, queryTime: perf.QueryTime}, true
}

// Durations are cumulative 100 ns ticks. An interval without operations has no
// measured latency (not zero milliseconds). Counter reset/wrap needs a new base.
func diskLatency(beforeTime, afterTime uint64, beforeCount, afterCount uint32) *float64 {
	if afterTime < beforeTime || afterCount <= beforeCount {
		return nil
	}
	v := float64(afterTime-beforeTime) / float64(afterCount-beforeCount) / 10000
	return &v
}

func diskActivity(before, after physicalCounters) *float64 {
	if after.queryTime <= before.queryTime || after.idleTime < before.idleTime {
		return nil
	}
	elapsed := uint64(after.queryTime - before.queryTime)
	idle := after.idleTime - before.idleTime
	if idle > elapsed {
		return nil
	}
	v := 100 * (1 - float64(idle)/float64(elapsed))
	return &v
}
