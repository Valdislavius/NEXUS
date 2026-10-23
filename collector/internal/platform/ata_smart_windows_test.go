//go:build windows

package platform

import (
	"encoding/binary"
	"errors"
	"nexus/collector/internal/model"
	"testing"
	"unsafe"

	"golang.org/x/sys/windows"
)

func fixtureSMART(entries ...[]byte) []byte {
	data := make([]byte, 512)
	binary.LittleEndian.PutUint16(data, 1)
	for i, entry := range entries {
		copy(data[2+i*12:], entry)
	}
	fixSMARTChecksum(data)
	return data
}
func fixSMARTChecksum(data []byte) {
	data[511] = 0
	var sum byte
	for _, b := range data[:511] {
		sum += b
	}
	data[511] = -sum
}
func fixtureAttribute(id byte, current byte, raw uint64) []byte {
	entry := make([]byte, 12)
	entry[0], entry[1], entry[3], entry[4] = id, 0x32, current, current-1
	for i := 0; i < 6; i++ {
		entry[5+i] = byte(raw >> (8 * i))
	}
	return entry
}
func healthMetric(result diskHealthResult, id string) *model.Metric {
	for i := range result.metrics {
		if result.metrics[i].ID == id {
			return &result.metrics[i]
		}
	}
	return nil
}
func errorCode(t *testing.T, err error, expected string) {
	t.Helper()
	if err == nil || ataErrorDiagnostic(err).code != expected {
		t.Fatalf("error = %v, expected %s", err, expected)
	}
}
func TestATAReadOnlyCommandListAndNativeLayout(t *testing.T) {
	for _, command := range [][2]byte{{ataIdentify, 0}, {ataSMART, ataSMARTReadData}, {ataSMART, ataSMARTThresholds}} {
		p, err := newATAReadPacket(command[0], command[1])
		if err != nil {
			t.Fatal(err)
		}
		expectedSize := uintptr(40)
		if unsafe.Sizeof(uintptr(0)) == 8 {
			expectedSize = 48
		}
		if unsafe.Sizeof(p.Header) != expectedSize || p.Header.DataOffset != expectedSize || p.Header.Flags != 2 || p.Header.Transfer != 512 || p.Header.Timeout != 3 {
			t.Fatalf("incorrect Windows layout/flags: %#v", p.Header)
		}
		if p.Header.Current[6] != command[0] || p.Header.Current[0] != command[1] {
			t.Fatal("incorrect taskfile command")
		}
		if command[0] == ataSMART && (p.Header.Current[3] != 0x4f || p.Header.Current[4] != 0xc2 || p.Header.Current[1] != 1) {
			t.Fatal("incorrect SMART signature")
		}
	}
	for _, command := range [][2]byte{{0x30, 0}, {ataSMART, 0xd8}, {ataSMART, 0xd9}, {ataSMART, 0xd4}, {ataSMART, 0xd6}, {ataSMART, 0xda}, {ataIdentify, 1}} {
		_, err := newATAReadPacket(command[0], command[1])
		errorCode(t, err, "ATA_COMMAND_NOT_ALLOWED")
	}
}
func TestATAReplyRejectsDriverAndDeviceErrors(t *testing.T) {
	valid, _ := newATAReadPacket(ataSMART, ataSMARTReadData)
	valid.Header.Current[6], valid.Header.Current[0] = 0x50, 0
	valid.Data[0] = 7
	fullSize := uint32(unsafe.Sizeof(valid))
	data, err := validateATAReply(&valid, fullSize)
	if err != nil || len(data) != 512 || data[0] != 7 {
		t.Fatalf("valid reply %v", err)
	}
	for _, tc := range []struct {
		name string
		edit func(*ataReadPacket)
		size uint32
		code string
	}{
		{"short header", func(*ataReadPacket) {}, 20, "ATA_SHORT_RESPONSE"},
		{"short sector", func(*ataReadPacket) {}, fullSize - 1, "ATA_SHORT_RESPONSE"},
		{"overrun", func(*ataReadPacket) {}, fullSize + 1, "ATA_INVALID_LAYOUT"},
		{"offset", func(p *ataReadPacket) { p.Header.DataOffset++ }, fullSize, "ATA_INVALID_LAYOUT"},
		{"length", func(p *ataReadPacket) { p.Header.Length-- }, fullSize, "ATA_INVALID_LAYOUT"},
		{"transfer", func(p *ataReadPacket) { p.Header.Transfer-- }, fullSize, "ATA_SHORT_RESPONSE"},
		{"busy", func(p *ataReadPacket) { p.Header.Current[6] = 0x80 }, fullSize, "ATA_INVALID_STATUS"},
		{"zero status", func(p *ataReadPacket) { p.Header.Current[6] = 0 }, fullSize, "ATA_INVALID_STATUS"},
		{"DRQ", func(p *ataReadPacket) { p.Header.Current[6] = 0x48 }, fullSize, "ATA_INVALID_STATUS"},
		{"ABRT with header only", func(p *ataReadPacket) { p.Header.Current[6], p.Header.Current[0] = 0x51, 0x04 }, uint32(unsafe.Sizeof(valid.Header)), "ATA_STATUS_51_ERROR_04"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			p := valid
			tc.edit(&p)
			_, err := validateATAReply(&p, tc.size)
			errorCode(t, err, tc.code)
		})
	}
}
func TestATASMARTSamsungCountersAndNoGuessedHealth(t *testing.T) {
	data := fixtureSMART(fixtureAttribute(9, 99, 12345), fixtureAttribute(12, 99, 456), fixtureAttribute(177, 97, 52), fixtureAttribute(190, 65, 0x003d0022), fixtureAttribute(241, 99, 12345678901))
	threshold := make([]byte, 12)
	threshold[0], threshold[1] = 177, 10
	result, err := parseATASMART(data, fixtureSMART(threshold), "Samsung SSD 860 EVO 1TB", false)
	if err != nil || len(result.attributes) != 5 {
		t.Fatalf("decode: %v", err)
	}
	for id, expected := range map[string]float64{"power-hours": 12345, "power-cycles": 456, "wear-leveling": 52, "temperature": 34, "lifetime-written": 12345678901 * 512} {
		m := healthMetric(result, id)
		if m == nil || m.Value == nil || *m.Value != expected {
			t.Errorf("%s = %#v, expected %v", id, m, expected)
		}
	}
	a := result.attributes[2]
	if a.Current != 97 || a.Worst != 96 || a.Threshold == nil || *a.Threshold != 10 || a.RawValue != "52" || a.RawHex != "000000000034" {
		t.Fatalf("RAW/normalized/threshold mixed: %#v", a)
	}
	if healthMetric(result, "remaining-life") != nil || healthMetric(result, "wear") != nil || healthMetric(result, "health") != nil {
		t.Fatal("invented generic health percentage")
	}
}
func TestATASMARTUnknownVendorPreservesExactRAW(t *testing.T) {
	result, err := parseATASMART(fixtureSMART(fixtureAttribute(9, 73, 0xffffffffffff), fixtureAttribute(241, 90, 77), fixtureAttribute(231, 100, 99)), nil, "UNKNOWN SATA SSD", false)
	if err != nil || len(result.attributes) != 3 || len(result.metrics) != 0 {
		t.Fatalf("unknown vendor got semantic metrics: %#v, %v", result, err)
	}
	a := result.attributes[0]
	if a.RawHex != "FFFFFFFFFFFF" || a.RawValue != "281474976710655" || a.Value != nil || a.Threshold != nil || a.Current != 73 {
		t.Fatalf("lost/misinterpreted raw: %#v", a)
	}
}
func TestATASMARTCrucialLifeUsesCurrentAndKeepsLBAUnits(t *testing.T) {
	result, err := parseATASMART(fixtureSMART(fixtureAttribute(5, 100, 3), fixtureAttribute(194, 66, 0x003d00120022), fixtureAttribute(202, 81, 19), fixtureAttribute(246, 100, 123)), nil, "Crucial_CT500MX200SSD1", false)
	if err != nil {
		t.Fatal(err)
	}
	if m := healthMetric(result, "remaining-life"); m == nil || *m.Value != 81 || m.Unit != "%" {
		t.Fatal("used RAW as remaining life")
	}
	if m := healthMetric(result, "total-lbas-written"); m == nil || *m.Value != 123 || m.Unit != "LBA" {
		t.Fatal("invented LBA byte unit")
	}
	if m := healthMetric(result, "temperature"); m == nil || *m.Value != 34 {
		t.Fatal("included historical min/max as temperature")
	}
	if m := healthMetric(result, "reallocated-blocks"); m == nil || *m.Value != 3 || healthMetric(result, "reallocated") != nil {
		t.Fatal("confused NAND blocks and HDD sectors")
	}
	// MX500 and BX500 are excluded from this document's family scope.
	unknown, err := parseATASMART(fixtureSMART(fixtureAttribute(202, 81, 19)), nil, "CT500MX500SSD1", false)
	if err != nil || healthMetric(unknown, "remaining-life") != nil {
		t.Fatal("applied undocumented family profile")
	}
}
func TestATASMARTKingstonNoUniversalF1OrE7RAWPercent(t *testing.T) {
	result, err := parseATASMART(fixtureSMART(fixtureAttribute(231, 91, 65535), fixtureAttribute(241, 100, 1000)), nil, "KINGSTON SA400S37480G", false)
	if err != nil {
		t.Fatal(err)
	}
	m := healthMetric(result, "wear-indicator")
	if m == nil || *m.Value != 91 || m.Unit == "%" || healthMetric(result, "lifetime-written") != nil || result.attributes[1].Value != nil {
		t.Fatal("Kingston units were guessed")
	}
}
func TestATASMARTWDHDDAndSSDRemainDistinct(t *testing.T) {
	data := fixtureSMART(fixtureAttribute(5, 100, 0x123400000007), fixtureAttribute(197, 100, 2), fixtureAttribute(198, 100, 0))
	hdd, err := parseATASMART(data, nil, "WDC WD10EZEX-00RKKA0", true)
	if err != nil || *healthMetric(hdd, "reallocated").Value != 7 || *healthMetric(hdd, "pending-sectors").Value != 2 || *healthMetric(hdd, "offline-uncorrectable").Value != 0 {
		t.Fatal("HDD counters")
	}
	ssd, err := parseATASMART(data, nil, "WDC WDS100T2B0A-00SM50", false)
	if err != nil || len(ssd.metrics) != 0 {
		t.Fatal("applied HDD semantics to SSD")
	}
}
func TestATASMARTMalformedDataAndOptionalThresholds(t *testing.T) {
	valid := fixtureSMART(fixtureAttribute(9, 99, 0))
	for _, tc := range []struct {
		name string
		data []byte
		code string
	}{
		{"short", valid[:511], "ATA_SHORT_RESPONSE"},
		{"all zero", make([]byte, 512), "ATA_SMART_REVISION"},
		{"empty", fixtureSMART(), "ATA_SMART_EMPTY"},
		{"duplicate", fixtureSMART(fixtureAttribute(9, 99, 1), fixtureAttribute(9, 99, 2)), "ATA_SMART_DUPLICATE_ID"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			_, err := parseATASMART(tc.data, nil, "Samsung SSD 860 EVO 1TB", false)
			errorCode(t, err, tc.code)
		})
	}
	broken := append([]byte(nil), valid...)
	broken[5]++
	_, err := parseATASMART(broken, nil, "Samsung SSD 860 EVO 1TB", false)
	errorCode(t, err, "ATA_SMART_CHECKSUM")
	result, err := parseATASMART(valid, broken, "Samsung SSD 860 EVO 1TB", false)
	if err != nil || len(result.attributes) != 1 || result.attributes[0].Threshold != nil || *healthMetric(result, "power-hours").Value != 0 || healthMetric(result, "smart-thresholds").Value != nil {
		t.Fatal("bad optional thresholds discarded good data or invented threshold/zero")
	}
	invalidLife := fixtureSMART(fixtureAttribute(202, 199, 157), fixtureAttribute(194, 100, 0))
	result, err = parseATASMART(invalidLife, nil, "Crucial_CT256MX100SSD1", false)
	if err != nil || healthMetric(result, "remaining-life") != nil || healthMetric(result, "temperature") != nil {
		t.Fatal("accepted invalid percentage or zero temperature")
	}
}
func fixtureIdentity(name string) []byte {
	data := make([]byte, 512)
	binary.LittleEndian.PutUint16(data, 0x0040)
	binary.LittleEndian.PutUint16(data[164:], 1)
	binary.LittleEndian.PutUint16(data[170:], 1)
	for i := 54; i < 94; i += 2 {
		data[i], data[i+1] = ' ', ' '
	}
	for i, b := range []byte(name) {
		data[54+(i^1)] = b
	}
	return data
}

type fakeATAIO struct {
	opened, closed int
	reads          [][2]byte
	failOpen       error
	failRead       int
	data           [][]byte
}

func (f *fakeATAIO) open(uint32) (windows.Handle, error) {
	f.opened++
	if f.failOpen != nil {
		return 0, f.failOpen
	}
	return windows.Handle(7), nil
}
func (f *fakeATAIO) close(windows.Handle) { f.closed++ }
func (f *fakeATAIO) read(_ windows.Handle, cmd, sub byte) ([]byte, error) {
	f.reads = append(f.reads, [2]byte{cmd, sub})
	i := len(f.reads)
	if f.failRead == i {
		return nil, windows.ERROR_NOT_SUPPORTED
	}
	return f.data[i-1], nil
}
func TestATAHandlesCloseAndOptionalThresholdErrors(t *testing.T) {
	for _, failAt := range []int{1, 2, 3, 0} {
		device := &fakeATAIO{failRead: failAt, data: [][]byte{fixtureIdentity("Samsung SSD 860 EVO 1TB"), fixtureSMART(fixtureAttribute(9, 99, 12)), fixtureSMART()}}
		result, err := readATAHealth(0, device)
		if device.opened != 1 || device.closed != 1 || len(device.reads) > 3 {
			t.Fatal("leaked handle or unbounded retry")
		}
		if failAt == 1 || failAt == 2 {
			if err == nil || len(result.attributes) != 0 {
				t.Fatal("accepted failing required command")
			}
		} else if err != nil || len(result.attributes) != 1 {
			t.Fatalf("optional threshold failure lost data: %v", err)
		}
	}
	device := &fakeATAIO{failOpen: windows.ERROR_ACCESS_DENIED}
	_, err := readATAHealth(0, device)
	if !errors.Is(err, windows.ERROR_ACCESS_DENIED) || device.closed != 0 || len(device.reads) != 0 {
		t.Fatal("read after denied open")
	}
	device = &fakeATAIO{data: [][]byte{fixtureIdentity("Samsung SSD 860 EVO 1TB")}}
	device.data[0][170] = 0 // Disabled SMART must never be enabled by this adapter.
	_, err = readATAHealth(0, device)
	errorCode(t, err, "ATA_SMART_DISABLED")
	if device.closed != 1 || len(device.reads) != 1 {
		t.Fatal("commands issued after disabled SMART")
	}
}
func TestATADiagnosticCategories(t *testing.T) {
	for _, tc := range []struct {
		err    error
		status string
	}{
		{windows.ERROR_ACCESS_DENIED, "permission_denied"},
		{windows.ERROR_NOT_SUPPORTED, "driver_no_data"},
		{windows.ERROR_SEM_TIMEOUT, "device_unresponsive"},
		{windows.ERROR_IO_DEVICE, "device_unresponsive"},
		{windows.ERROR_BUSY, "transient_error"},
		{ataInvalid("ATA_SMART_CHECKSUM"), "invalid_response"},
	} {
		m := ataMissing("smart-data", "SMART", tc.err)
		if m.Value != nil || m.Diagnostic == nil || m.Diagnostic.Status != tc.status || m.Diagnostic.Code == "" {
			t.Fatalf("wrong null/category for %v: %#v", tc.err, m)
		}
	}
}
func TestDiskHealthFailureClearsRAWAndStaleValuesAndRecovers(t *testing.T) {
	c := model.New("disk-0", "storage", "Disk", "")
	c.Set(model.Known("capacity", "Capacity", "bytes", "Win32_DiskDrive", 1000, false))
	success := diskHealthResult{metrics: []model.Metric{model.Known("power-hours", "Hours", "hours", "ATA SMART", 200, false), model.Known("temperature", "Temperature", "°C", "Windows Storage", 34, true)}, attributes: []model.SMARTAttribute{{ID: 9, RawHex: "0000000000C8", RawValue: "200"}}}
	applyDiskHealthResult(&c, success)
	failure := diskHealthResult{metrics: []model.Metric{ataMissing("smart-data", "SMART", windows.ERROR_ACCESS_DENIED), model.Known("temperature", "Temperature", "°C", "Windows Storage", 35, true)}}
	applyDiskHealthResult(&c, failure)
	if len(c.SMART) != 0 {
		t.Fatal("stale SMART table retained")
	}
	for _, m := range c.Metrics {
		if m.ID == "power-hours" && (m.Value != nil || m.Diagnostic.Status != "permission_denied") {
			t.Fatal("stale counter or wrong failure reason")
		}
		if m.ID == "temperature" && (m.Value == nil || *m.Value != 35) {
			t.Fatal("existing Windows temperature lost on ATA access denial")
		}
		if m.ID == "capacity" && (m.Value == nil || *m.Value != 1000) {
			t.Fatal("non-health metric lost")
		}
	}
	applyDiskHealthResult(&c, success)
	for _, m := range c.Metrics {
		if m.ID == "smart-data" {
			t.Fatal("successful recovery retained old failure")
		}
	}
}
