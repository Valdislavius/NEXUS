//go:build windows

package platform

import (
	"golang.org/x/sys/windows"
	"nexus/collector/internal/model"
	"unsafe"
)

// SIZE_T fields are pointer-sized. One unprivileged native read supplies physical
// memory, commit, cache and process counts; no WMI or per-process enumeration.
// https://learn.microsoft.com/windows/win32/api/psapi/ns-psapi-performance_information
type performanceInformation struct {
	Size                                               uint32
	CommitTotal, CommitLimit, CommitPeak               uintptr
	PhysicalTotal, PhysicalAvailable, SystemCache      uintptr
	KernelTotal, KernelPaged, KernelNonpaged, PageSize uintptr
	Handles, Processes, Threads                        uint32
}

var getPerformanceInfo = windows.NewLazySystemDLL("psapi.dll").NewProc("GetPerformanceInfo")

func readPerformance() (performanceInformation, bool) {
	p := performanceInformation{}
	p.Size = uint32(unsafe.Sizeof(p))
	r, _, _ := getPerformanceInfo.Call(uintptr(unsafe.Pointer(&p)), uintptr(p.Size))
	return p, r != 0 && p.PageSize > 0 && p.PhysicalTotal > 0 && p.PhysicalAvailable <= p.PhysicalTotal && p.CommitTotal <= p.CommitLimit
}

func applyMemory(c *model.Component, p performanceInformation, valid bool) {
	definitions := []struct {
		id, label, unit string
		primary         bool
		value           uintptr
	}{
		{"used", "Используется", "bytes", true, p.PhysicalTotal - p.PhysicalAvailable},
		{"available", "Доступно", "bytes", true, p.PhysicalAvailable},
		{"total", "Доступно Windows всего", "bytes", false, p.PhysicalTotal},
		{"commit", "Committed memory", "bytes", false, p.CommitTotal},
		{"commitLimit", "Лимит commit", "bytes", false, p.CommitLimit},
		{"commitPeak", "Пик commit после загрузки", "bytes", false, p.CommitPeak},
		{"cache", "Системный кэш", "bytes", false, p.SystemCache},
		{"kernelPaged", "Выгружаемый пул ядра", "bytes", false, p.KernelPaged},
		{"kernelNonpaged", "Невыгружаемый пул ядра", "bytes", false, p.KernelNonpaged},
	}
	for _, d := range definitions {
		if valid {
			c.Set(model.Known(d.id, d.label, d.unit, "GetPerformanceInfo", float64(d.value)*float64(p.PageSize), d.primary))
		} else {
			c.Set(model.MissingDiagnostic(d.id, d.label, d.unit, "GetPerformanceInfo", "Счётчики памяти Windows недоступны", "driver_no_data", "MEMORY_COUNTER_UNAVAILABLE", d.primary))
		}
	}
	for _, d := range []struct {
		id, label   string
		used, total uintptr
		primary     bool
	}{
		{"load", "Загрузка", p.PhysicalTotal - p.PhysicalAvailable, p.PhysicalTotal, true},
		{"commitLoad", "Использование commit", p.CommitTotal, p.CommitLimit, false},
	} {
		if valid && d.total > 0 {
			c.Set(model.Known(d.id, d.label, "%", "GetPerformanceInfo", 100*float64(d.used)/float64(d.total), d.primary))
		} else {
			c.Set(model.Missing(d.id, d.label, "%", "GetPerformanceInfo", "Счётчики памяти Windows недоступны", d.primary))
		}
	}
}
