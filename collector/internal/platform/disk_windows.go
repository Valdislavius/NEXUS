//go:build windows

package platform

import (
	"fmt"
	"golang.org/x/sys/windows"
	"unsafe"
)

type physicalCounters struct{ read, write uint64 }

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
	if err != nil || returned < uint32(unsafe.Sizeof(perf)) || perf.BytesRead < 0 || perf.BytesWritten < 0 {
		return physicalCounters{}, false
	}
	return physicalCounters{uint64(perf.BytesRead), uint64(perf.BytesWritten)}, true
}
