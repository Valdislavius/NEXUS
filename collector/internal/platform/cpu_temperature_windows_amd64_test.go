//go:build windows && amd64

package platform

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"golang.org/x/sys/windows"
	"nexus/collector/internal/model"
)

var resultPath = flag.String("nexus-result", "", "Write read-only CPU diagnostic samples")

func TestPackageTemperatureDecode(t *testing.T) {
	for _, c := range []struct {
		name           string
		target, status uint64
		want           float64
		ok             bool
	}{
		{"12600KF sample", 100 << 16, 1<<31 | 66<<16, 34, true},
		{"normal", 100 << 16, 1<<31 | 62<<16, 38, true},
		{"zero delta", 100 << 16, 1 << 31, 100, true},
		{"real target 91", 91 << 16, 1<<31 | 66<<16, 25, true},
		{"target lower bound", 50 << 16, 1<<31 | 50<<16, 0, true},
		{"target upper bound", 125 << 16, 1<<31 | 127<<16, 0, false},
		{"missing target", 0, 1 << 31, 0, false},
		{"low invalid target", 49 << 16, 1 << 31, 0, false},
		{"high invalid target", 126 << 16, 1 << 31, 0, false},
		{"validity absent", 100 << 16, 62 << 16, 0, false},
		{"negative result", 100 << 16, 1<<31 | 110<<16, 0, false},
		{"reserved bits ignored", uint64(1)<<63 | 100<<16 | 7<<24, uint64(1)<<63 | 1<<31 | 62<<16 | 1<<23, 38, true},
	} {
		t.Run(c.name, func(t *testing.T) {
			got, err := decodePackageTemperature(c.target, c.status)
			if (err == nil) != c.ok || (c.ok && got != c.want) {
				t.Fatalf("%+v: %v %v", c, got, err)
			}
		})
	}
}

func TestPackageHardware(t *testing.T) {
	s := cpuThermometer{}
	defer s.close()
	rows := []map[string]any{}
	count := 1
	if *resultPath != "" {
		count = 5
	}
	for i := 0; i < count; i++ {
		v, err := s.temperature()
		var reading *float64
		message, status, code := "", "available", ""
		if err != nil {
			message = err.Error()
			status, code = cpuTemperatureDiagnostic(err)
		} else {
			reading = &v
		}
		rows = append(rows, map[string]any{"at": time.Now().UTC(), "celsius": reading, "reason": message, "status": status, "code": code, "sensor": s.source()})
		t.Logf("CPU temperature: %v C, status: %s, code: %s, reason: %s", reading, status, code, message)
		if i < count-1 {
			time.Sleep(time.Second)
		}
	}
	if *resultPath != "" {
		b, err := json.MarshalIndent(rows, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(*resultPath, b, 0600); err != nil {
			t.Fatal(err)
		}
	}
}

func TestMSRReadAllowlist(t *testing.T) {
	for _, index := range []uint64{0, 0x198, 0x19c, 0x1a4, 0xffffffff, 0x1000001a2} {
		if _, err := (nativeIntelTemperatureDriver{}).read(0, index); err == nil {
			t.Fatalf("unapproved MSR %x must be rejected before driver access", index)
		} else if status, _ := cpuTemperatureDiagnostic(err); status != "unsupported" {
			t.Fatalf("unexpected status %s", status)
		}
	}
}

func cpuFixture(vendor string, signature, max, features uint32) cpuidReader {
	return func(leaf, subleaf uint32) (a, b, c, d uint32) {
		switch leaf {
		case 0:
			return max, binary.LittleEndian.Uint32([]byte(vendor[0:4])), binary.LittleEndian.Uint32([]byte(vendor[8:12])), binary.LittleEndian.Uint32([]byte(vendor[4:8]))
		case 1:
			return signature, 0, 0, 0
		case 6:
			return features, 0, 0, 0
		}
		return
	}
}

func TestCPUIdentity(t *testing.T) {
	for _, c := range []struct {
		vendor                             string
		signature, family, model, stepping uint32
	}{
		{"GenuineIntel", 0x90672, 6, 0x97, 2},
		{"AuthenticAMD", 0x800f11, 0x17, 0x1, 1},
		{"AuthenticAMD", 0xa20f12, 0x19, 0x21, 2},
		{"AuthenticAMD", 0xb40f40, 0x1a, 0x44, 0},
	} {
		id := identifyCPU(cpuFixture(c.vendor, c.signature, 6, 1<<6))
		if id.vendor != c.vendor || id.family != c.family || id.model != c.model || id.stepping != c.stepping {
			t.Fatalf("%+v => %+v", c, id)
		}
	}
}

func TestAMDAdapterNeverSelectsIntel(t *testing.T) {
	for _, signature := range []uint32{0x800f11, 0xa20f12, 0xb40f40, 0x600f10} {
		s := cpuThermometer{readCPU: cpuFixture("AuthenticAMD", signature, 6, 1<<6)}
		value, err := s.temperature()
		if _, ok := s.adapter.(*amdCPUThermometer); !ok {
			t.Fatal("AMD dispatched to a non-AMD adapter")
		}
		status, code := cpuTemperatureDiagnostic(err)
		want := "AMD_SMN_SELECTOR_WRITE_REQUIRED"
		if identifyCPU(s.readCPU).family < 0x17 {
			want = "AMD_FAMILY_UNVERIFIED"
		}
		if value != 0 || err == nil || status != "unsupported" || code != want {
			t.Fatalf("%x: value=%v status=%s code=%s err=%v", signature, value, status, code, err)
		}
		m := sampleCPUTemperature(&s, true)
		b, _ := json.Marshal(m)
		if m.Value != nil || !strings.Contains(string(b), `"value":null`) {
			t.Fatalf("AMD must publish null: %s", b)
		}
		s.close()
	}
}

type fakeIntelTemperatureDriver struct {
	opens, loads, closes int
	reads                []uint64
	openErr, loadErr     error
	readErr              error
	errorIndex           uint64
	target, status       uint64
}

func (d *fakeIntelTemperatureDriver) open() (windows.Handle, error) {
	d.opens++
	if d.openErr != nil {
		return 0, d.openErr
	}
	return windows.Handle(d.opens), nil
}
func (d *fakeIntelTemperatureDriver) load(windows.Handle) error { d.loads++; return d.loadErr }
func (d *fakeIntelTemperatureDriver) close(windows.Handle)      { d.closes++ }
func (d *fakeIntelTemperatureDriver) read(h windows.Handle, index uint64) (uint64, error) {
	d.reads = append(d.reads, index)
	if d.readErr != nil && index == d.errorIndex {
		return 0, d.readErr
	}
	if index == msrTemperatureTarget {
		return d.target, nil
	}
	if index == msrPackageThermalStatus {
		return d.status, nil
	}
	return 0, errors.New("test saw an unapproved register")
}
func intelFixture(d *fakeIntelTemperatureDriver, now *time.Time) *intelCPUThermometer {
	return &intelCPUThermometer{driver: d, readCPU: cpuFixture("GenuineIntel", 0x90672, 6, 1<<6), now: func() time.Time { return *now }, bindThread: func() (func(), error) { return func() {}, nil }}
}
func requireCPUFailure(t *testing.T, got float64, err error, status string) {
	t.Helper()
	actual, _ := cpuTemperatureDiagnostic(err)
	if got != 0 || err == nil || actual != status {
		t.Fatalf("got value=%v, status=%s, error=%v; want unavailable %s", got, actual, err, status)
	}
}

func TestIntelHandleReuseCloseAndReadAllowlist(t *testing.T) {
	now := time.Unix(100, 0)
	d := &fakeIntelTemperatureDriver{target: 100 << 16, status: 1<<31 | 66<<16}
	s := intelFixture(d, &now)
	for i := 0; i < 3; i++ {
		if v, err := s.temperature(); err != nil || v != 34 {
			t.Fatalf("%v %v", v, err)
		}
	}
	if d.opens != 1 || d.loads != 1 || len(d.reads) != 6 {
		t.Fatalf("driver counts: %+v", d)
	}
	for i, index := range d.reads {
		if index != []uint64{msrTemperatureTarget, msrPackageThermalStatus}[i%2] {
			t.Fatalf("unexpected register %x", index)
		}
	}
	s.close()
	s.close()
	if d.closes != 1 || s.handle != 0 {
		t.Fatalf("close must release exactly once: %+v", d)
	}
}

func TestIntelFailureBackoffAndRecovery(t *testing.T) {
	for _, c := range []struct {
		name, stage string
		errorIndex  uint64
	}{
		{"open", "open", 0}, {"module", "load", 0}, {"target", "read_target", msrTemperatureTarget}, {"status", "read_status", msrPackageThermalStatus},
	} {
		t.Run(c.name, func(t *testing.T) {
			now := time.Unix(100, 0)
			d := &fakeIntelTemperatureDriver{target: 100 << 16, status: 1<<31 | 66<<16}
			switch c.stage {
			case "open":
				d.openErr = windows.ERROR_ACCESS_DENIED
			case "load":
				d.loadErr = windows.ERROR_ACCESS_DENIED
			default:
				d.readErr = windows.ERROR_ACCESS_DENIED
				d.errorIndex = c.errorIndex
			}
			s := intelFixture(d, &now)
			value, err := s.temperature()
			requireCPUFailure(t, value, err, "permission_denied")
			if s.handle != 0 {
				t.Fatal("failed read/load leaked handle")
			}
			if c.stage != "open" && d.closes != 1 {
				t.Fatalf("open handle not closed after %s failure: %+v", c.stage, d)
			}
			for i := 0; i < 100; i++ {
				value, err = s.temperature()
				requireCPUFailure(t, value, err, "permission_denied")
			}
			if d.opens != 1 {
				t.Fatalf("device reopened during backoff: %d", d.opens)
			}
			d.openErr, d.loadErr, d.readErr = nil, nil, nil
			now = now.Add(cpuOpenRetryInterval)
			value, err = s.temperature()
			if err != nil || value != 34 || d.opens != 2 {
				t.Fatalf("recovery failed: %v %v %+v", value, err, d)
			}
			s.close()
			wantCloses := 2
			if c.stage == "open" {
				wantCloses = 1
			}
			if d.closes != wantCloses {
				t.Fatalf("unbalanced handle lifetime: %+v", d)
			}
		})
	}
}

func TestIntelInvalidSampleClearsValueWithoutReopening(t *testing.T) {
	now := time.Unix(100, 0)
	d := &fakeIntelTemperatureDriver{target: 100 << 16, status: 1<<31 | 66<<16}
	intel := intelFixture(d, &now)
	s := cpuThermometer{readCPU: intel.readCPU, newAdapter: func(cpuIdentity, cpuidReader) cpuTemperatureAdapter { return intel }}
	c := model.New("cpu", "cpu", "CPU", "")
	c.Set(sampleCPUTemperature(&s, true))
	if c.Metrics[0].Value == nil || *c.Metrics[0].Value != 34 {
		t.Fatal("initial valid value missing")
	}
	d.status &^= 1 << 31
	c.Set(sampleCPUTemperature(&s, true))
	if c.Metrics[0].Value != nil || c.Metrics[0].Diagnostic.Status != "invalid_response" {
		t.Fatal("invalid sample retained stale temperature")
	}
	if d.opens != 1 || d.closes != 0 {
		t.Fatal("valid handle unnecessarily reopened")
	}
	d.status |= 1 << 31
	c.Set(sampleCPUTemperature(&s, true))
	if c.Metrics[0].Value == nil || c.Metrics[0].Diagnostic != nil {
		t.Fatal("sensor did not recover on next read")
	}
	s.close()
}

func TestIntelReopenBudgetIsFiniteUntilRedetection(t *testing.T) {
	now := time.Unix(100, 0)
	d := &fakeIntelTemperatureDriver{openErr: windows.ERROR_ACCESS_DENIED}
	s := intelFixture(d, &now)
	for i := 0; i < 20; i++ {
		v, err := s.temperature()
		requireCPUFailure(t, v, err, "permission_denied")
		now = now.Add(cpuOpenRetryInterval)
	}
	if d.opens != maxCPUOpenAttempts {
		t.Fatalf("reopen budget exceeded: %d", d.opens)
	}
	// Explicit hardware redetection creates a fresh adapter and permits retry.
	d.openErr = nil
	d.target, d.status = 100<<16, 1<<31|66<<16
	fresh := intelFixture(d, &now)
	if v, err := fresh.temperature(); err != nil || v != 34 {
		t.Fatalf("redetection did not recover: %v %v", v, err)
	}
	fresh.close()
}

func TestCPUHardwareRedetectionClosesOldAdapter(t *testing.T) {
	now := time.Unix(100, 0)
	d := &fakeIntelTemperatureDriver{target: 100 << 16, status: 1<<31 | 66<<16}
	intel := intelFixture(d, &now)
	vendor := "GenuineIntel"
	read := func(leaf, subleaf uint32) (uint32, uint32, uint32, uint32) {
		return cpuFixture(vendor, 0x800f11, 6, 1<<6)(leaf, subleaf)
	}
	s := cpuThermometer{readCPU: read, newAdapter: func(id cpuIdentity, read cpuidReader) cpuTemperatureAdapter {
		if id.vendor == "GenuineIntel" {
			return intel
		}
		return &amdCPUThermometer{identity: id}
	}}
	if _, err := s.temperature(); err != nil {
		t.Fatal(err)
	}
	vendor = "AuthenticAMD"
	v, err := s.temperature()
	requireCPUFailure(t, v, err, "unsupported")
	if d.closes != 1 || d.opens != 1 {
		t.Fatal("rediscovery did not close Intel handle or attempted new Intel open")
	}
	s.close()
	s.close()
	if d.closes != 1 {
		t.Fatal("rediscovery caused double close")
	}
}

func TestIntelCPUIDAndAffinityFailurePreventPrivilegedAccess(t *testing.T) {
	for _, c := range []struct {
		name    string
		read    cpuidReader
		bindErr error
		status  string
	}{
		{"AMD through Intel adapter", cpuFixture("AuthenticAMD", 0x800f11, 6, 1<<6), nil, "unsupported"},
		{"no leaf 6", cpuFixture("GenuineIntel", 0x90672, 1, 1<<6), nil, "unsupported"},
		{"no package feature", cpuFixture("GenuineIntel", 0x90672, 6, 0), nil, "unsupported"},
		{"affinity error", cpuFixture("GenuineIntel", 0x90672, 6, 1<<6), newCPUSensorError("transient_error", "CPU_AFFINITY_SET_FAILED", "affinity unavailable", nil), "transient_error"},
	} {
		t.Run(c.name, func(t *testing.T) {
			now := time.Unix(100, 0)
			d := &fakeIntelTemperatureDriver{}
			s := intelFixture(d, &now)
			s.readCPU = c.read
			if c.bindErr != nil {
				s.bindThread = func() (func(), error) { return nil, c.bindErr }
			}
			v, err := s.temperature()
			requireCPUFailure(t, v, err, c.status)
			if d.opens != 0 || len(d.reads) != 0 {
				t.Fatal("unsupported CPU reached privileged driver access")
			}
		})
	}
}

func TestCPUDriverErrorCategories(t *testing.T) {
	for _, c := range []struct {
		stage        string
		err          error
		status, code string
	}{
		{"open", windows.ERROR_ACCESS_DENIED, "permission_denied", "WIN32_5"},
		{"open", fmt.Errorf("wrapped: %w", windows.ERROR_FILE_NOT_FOUND), "dependency_missing", "WIN32_2"},
		{"open", windows.ERROR_PATH_NOT_FOUND, "dependency_missing", "WIN32_3"},
		{"read_status", windows.ERROR_DEVICE_NOT_CONNECTED, "device_unresponsive", "WIN32_1167"},
		{"read_status", windows.ERROR_NOT_SUPPORTED, "driver_no_data", "WIN32_50"},
		{"read_status", windows.ERROR_INVALID_DATA, "invalid_response", "WIN32_13"},
		{"read_status", windows.ERROR_OPERATION_ABORTED, "transient_error", "WIN32_995"},
		{"load", errors.New("secret path C:/Users/example"), "driver_no_data", "PAWNIO_LOAD_FAILED"},
	} {
		err := classifyCPUDriverError(c.stage, c.err)
		status, code := cpuTemperatureDiagnostic(err)
		if status != c.status || code != c.code || strings.Contains(err.Error(), "secret") || strings.Contains(err.Error(), "example") {
			t.Fatalf("%+v: %s %s %v", c, status, code, err)
		}
		if !errors.Is(err, c.err) {
			t.Fatal("internal cause lost")
		}
	}
}

func TestMSRResponseLength(t *testing.T) {
	for _, size := range []uint32{0, 4, 7, 9, 16} {
		if _, err := decodeMSRResponse(make([]byte, 8), size); err == nil {
			t.Fatalf("accepted %d-byte response", size)
		}
	}
	if _, err := decodeMSRResponse(make([]byte, 7), 8); err == nil {
		t.Fatal("accepted a short buffer")
	}
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, 0x12345678abcd)
	if v, err := decodeMSRResponse(b, 8); err != nil || v != 0x12345678abcd {
		t.Fatalf("%x %v", v, err)
	}
}

func TestMultipleCPUsClearAndCloseTemperature(t *testing.T) {
	now := time.Unix(100, 0)
	d := &fakeIntelTemperatureDriver{target: 100 << 16, status: 1<<31 | 66<<16}
	intel := intelFixture(d, &now)
	s := cpuThermometer{readCPU: intel.readCPU, newAdapter: func(cpuIdentity, cpuidReader) cpuTemperatureAdapter { return intel }}
	if m := sampleCPUTemperature(&s, true); m.Value == nil {
		t.Fatal("initial read missing")
	}
	m := sampleCPUTemperature(&s, false)
	if m.Value != nil || m.Diagnostic.Code != "CPU_MULTIPLE_PACKAGES_UNSUPPORTED" || d.closes != 1 {
		t.Fatalf("multiple CPU sample: %+v driver %+v", m, d)
	}
}
