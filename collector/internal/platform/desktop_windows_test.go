//go:build windows

package platform

import (
	"nexus/collector/internal/model"
	"testing"
	"time"
	"unsafe"
)

func metricByID(t *testing.T, c model.Component, id string) model.Metric {
	t.Helper()
	for _, m := range c.Metrics {
		if m.ID == id {
			return m
		}
	}
	t.Fatalf("missing metric %s", id)
	return model.Metric{}
}

func TestMemoryUsesPageUnitsAndClearsFailedSample(t *testing.T) {
	c := model.New("ram", "ram", "RAM", "")
	p := performanceInformation{PhysicalTotal: 100, PhysicalAvailable: 25, CommitTotal: 150, CommitLimit: 200, SystemCache: 12, PageSize: 4096}
	applyMemory(&c, p, true)
	for id, expected := range map[string]float64{"used": 75 * 4096, "available": 25 * 4096, "cache": 12 * 4096, "commit": 150 * 4096, "commitLimit": 200 * 4096, "load": 75, "commitLoad": 75} {
		m := metricByID(t, c, id)
		if m.Value == nil || *m.Value != expected || m.Source != "GetPerformanceInfo" {
			t.Fatalf("%s %+v", id, m)
		}
	}
	applyMemory(&c, performanceInformation{}, false)
	for _, m := range c.Metrics {
		if m.Value != nil || m.Status != "unavailable" {
			t.Fatalf("old sample retained: %+v", m)
		}
	}
}

func TestNativePerformanceLayout(t *testing.T) {
	if unsafe.Sizeof(uintptr(0)) == 8 && unsafe.Sizeof(performanceInformation{}) != 104 {
		t.Fatal("Windows x64 ABI mismatch")
	}
	if unsafe.Sizeof(diskPerformance{}) != 88 {
		t.Fatal("DISK_PERFORMANCE ABI mismatch")
	}
}

func TestDiskLatencyAndActivity(t *testing.T) {
	if v := diskLatency(100, 20100, 10, 12); v == nil || *v != 1 {
		t.Fatal("latency must be ms per completed operation")
	}
	if diskLatency(100, 200, 10, 10) != nil || diskLatency(100, 90, 10, 11) != nil || diskLatency(100, 200, 11, 10) != nil {
		t.Fatal("idle/reset counters fabricated a latency")
	}
	before := physicalCounters{queryTime: 10000000, idleTime: 100}
	after := physicalCounters{queryTime: 20000000, idleTime: 5000100}
	if v := diskActivity(before, after); v == nil || *v != 50 {
		t.Fatal("incorrect disk activity")
	}
	after.idleTime = 10000101
	if diskActivity(before, after) != nil || diskActivity(before, before) != nil {
		t.Fatal("counter reset/timing mismatch accepted")
	}
}

func TestFreshnessUsesSeparateSlowLane(t *testing.T) {
	now := time.Now()
	c := model.New("disk", "storage", "Disk", "")
	for _, source := range []string{"IOCTL_DISK_PERFORMANCE", "NVMe SMART"} {
		m := model.Known(source, source, "bytes", source, 5, false)
		m.At = now.Add(-10 * time.Second).Format(time.RFC3339Nano)
		c.Set(m)
	}
	p, _ := Profile("balanced")
	components := []model.Component{c}
	stampFreshness(components, now, p)
	if metricByID(t, components[0], "IOCTL_DISK_PERFORMANCE").Value != nil {
		t.Fatal("expired fast counter retained")
	}
	if metricByID(t, components[0], "NVMe SMART").Value == nil {
		t.Fatal("valid slow cache expired at fast cadence")
	}
}

func TestProfilesBoundPolling(t *testing.T) {
	for name, want := range map[string][2]time.Duration{"economy": {5 * time.Second, 60 * time.Second}, "balanced": {2 * time.Second, 30 * time.Second}, "detailed": {2 * time.Second, 10 * time.Second}} {
		p, ok := Profile(name)
		if !ok || p.Fast != want[0] || p.Slow != want[1] {
			t.Fatal(name, p)
		}
	}
	if _, ok := Profile("unbounded"); ok {
		t.Fatal("unknown polling profile accepted")
	}
}
