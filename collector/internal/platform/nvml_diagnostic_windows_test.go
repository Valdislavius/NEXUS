//go:build windows

package platform

import (
	"fmt"
	"strings"
	"testing"

	"nexus/collector/internal/model"
)

func TestNVMLDiagnosticCategories(t *testing.T) {
	for _, test := range []struct {
		code   uintptr
		status string
	}{
		{0, "invalid_response"}, {1, "driver_no_data"}, {2, "invalid_response"}, {3, "unsupported"},
		{4, "permission_denied"}, {5, "transient_error"}, {6, "driver_no_data"}, {7, "invalid_response"},
		{8, "device_unresponsive"}, {9, "dependency_missing"}, {10, "transient_error"}, {11, "device_unresponsive"},
		{12, "dependency_missing"}, {13, "dependency_missing"}, {14, "invalid_response"}, {15, "device_unresponsive"},
		{16, "device_unresponsive"}, {17, "permission_denied"}, {18, "dependency_missing"}, {19, "transient_error"},
		{20, "transient_error"}, {21, "driver_no_data"}, {22, "unsupported"}, {23, "transient_error"},
		{24, "unsupported"}, {25, "invalid_response"}, {26, "unsupported"}, {27, "transient_error"},
		{28, "driver_no_data"}, {29, "transient_error"}, {30, "unsupported"}, {999, "transient_error"}, {^uintptr(0), "transient_error"},
	} {
		t.Run(fmt.Sprint(test.code), func(t *testing.T) {
			diagnostic := nvmlDiagnostic(test.code)
			if diagnostic.Status != test.status {
				t.Fatalf("%d: got %s, want %s", test.code, diagnostic.Status, test.status)
			}
			metric := nvmlMissing("temperature", "Температура", "°C", true, diagnostic)
			if metric.Value != nil || metric.Diagnostic == nil || strings.Contains(metric.Reason, "NVML") {
				t.Fatalf("unsafe missing metric: %+v", metric)
			}
		})
	}
	if got := nvmlDiagnostic(^uintptr(0)).Code; got != "NVML_999" {
		t.Fatalf("unbounded code: %s", got)
	}
}

func TestNVMLInvalidAndRealScalarValues(t *testing.T) {
	for _, value := range []uint32{256, ^uint32(0)} {
		metric := nvmlScalarMetric("temperature", "Температура", "°C", value, 1, true, 0)
		if metric.Value != nil || metric.Diagnostic.Status != "invalid_response" || metric.Diagnostic.Code != "NVML_VALUE_INVALID" {
			t.Fatalf("invalid reading survived: %+v", metric)
		}
	}
	for _, test := range []struct {
		id          string
		value       uint32
		scale, want float64
	}{{"temperature", 34, 1, 34}, {"power", 35000, 1000, 35}, {"fan", 120, 1, 120}, {"fan", 0, 1, 0}} {
		metric := nvmlScalarMetric(test.id, test.id, "", test.value, test.scale, false, 0)
		if metric.Value == nil || *metric.Value != test.want || metric.Diagnostic != nil {
			t.Fatalf("real reading changed: %+v", metric)
		}
	}
}

func TestNVMLFailureReplacesPreviousReadings(t *testing.T) {
	component := model.New("nv-0", "gpu", "NVIDIA GPU", "")
	for _, id := range []string{"temperature", "load", "memoryUsed", "memoryTotal"} {
		component.Set(model.Known(id, id, "", "NVIDIA NVML", 42, true))
	}
	adapter := &nvml{handles: []uintptr{1}, invoke: func(string, ...uintptr) uintptr { return 15 }}
	adapter.sample(&component)
	for _, metric := range component.Metrics {
		if metric.Value != nil || metric.Diagnostic == nil || metric.Diagnostic.Status != "device_unresponsive" || metric.Diagnostic.Code != "NVML_15" {
			t.Fatalf("old/incorrect reading: %+v", metric)
		}
	}
}

func TestNVMLMalformedMemorySuccessStaysNull(t *testing.T) {
	component := model.New("nv-0", "gpu", "NVIDIA GPU", "")
	component.Set(model.Known("memoryUsed", "Видеопамять", "bytes", "NVIDIA NVML", 42, true))
	adapter := &nvml{handles: []uintptr{1}, invoke: func(string, ...uintptr) uintptr { return 0 }}
	// A success response whose zero-initialized memory structure remains empty
	// is invalid: it must not replace unavailable memory with zero bytes.
	adapter.sample(&component)
	for _, metric := range component.Metrics {
		if metric.ID == "memoryUsed" || metric.ID == "memoryTotal" {
			if metric.Value != nil || metric.Diagnostic == nil || metric.Diagnostic.Status != "invalid_response" {
				t.Fatalf("malformed memory was accepted: %+v", metric)
			}
		}
	}
}

func TestNVMLInitializationFailureOnlyDiagnosesKnownPrimarySensors(t *testing.T) {
	for _, code := range []uintptr{4, 9, 12, 13, 15} {
		adapter := &nvml{failure: nvmlDiagnostic(code)}
		component := model.New("gpu-0", "gpu", "NVIDIA GeForce", "")
		gpuUnavailable(&component)
		adapter.sample(&component)
		for _, metric := range component.Metrics {
			if metric.Primary && metric.Diagnostic == nil {
				t.Fatalf("primary initialization reason missing: %+v", metric)
			}
			if !metric.Primary && metric.Diagnostic != nil {
				t.Fatalf("unqueried extra sensor advertised: %+v", metric)
			}
		}
	}
	other := model.New("gpu-1", "gpu", "AMD Radeon", "")
	gpuUnavailable(&other)
	(&nvml{failure: nvmlDiagnostic(12)}).sample(&other)
	for _, metric := range other.Metrics {
		if metric.Diagnostic != nil {
			t.Fatalf("NVML error attributed to another vendor: %+v", metric)
		}
	}
}
