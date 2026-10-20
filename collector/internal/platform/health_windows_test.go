//go:build windows

package platform

import (
	"encoding/binary"
	"os"
	"testing"

	"golang.org/x/sys/windows"
)

func TestNVMeMalformed(t *testing.T) {
	if parseNVMeHealth(make([]byte, 32)) != nil {
		t.Fatal("accepted short data")
	}
	d := make([]byte, 560)
	binary.LittleEndian.PutUint32(d[8:], 3)
	binary.LittleEndian.PutUint32(d[12:], 2)
	binary.LittleEndian.PutUint32(d[24:], 40)
	binary.LittleEndian.PutUint32(d[28:], 512)
	binary.LittleEndian.PutUint16(d[49:], 310)
	d[53] = 7
	m := parseNVMeHealth(d)
	if len(m) == 0 || *m[0].Value < 36 || *m[0].Value > 37 {
		t.Fatal("temperature decoding")
	}
	binary.LittleEndian.PutUint32(d[24:], ^uint32(0))
	if parseNVMeHealth(d) != nil {
		t.Fatal("accepted bad offset")
	}
}
func TestStorageTemperatureValidation(t *testing.T) {
	data := make([]byte, 40)
	binary.LittleEndian.PutUint16(data[12:], 1)
	binary.LittleEndian.PutUint16(data[26:], 34)
	if value := storageTemperature(data); value == nil || *value != 34 {
		t.Fatal("valid existing SATA temperature rejected")
	}
	for _, temperature := range []uint16{0, 151, 0x8000, 0xffff} {
		binary.LittleEndian.PutUint16(data[26:], temperature)
		if storageTemperature(data) != nil {
			t.Fatalf("accepted invalid temperature %d", temperature)
		}
	}
	binary.LittleEndian.PutUint16(data[26:], 34)
	binary.LittleEndian.PutUint16(data[24:], 1)
	if storageTemperature(data) != nil || storageTemperature(data[:39]) != nil {
		t.Fatal("accepted non-composite or short sensor")
	}
}
func TestStorageDeviceModelBoundsAndNoSerial(t *testing.T) {
	data := make([]byte, 140)
	binary.LittleEndian.PutUint32(data[4:], 140)
	binary.LittleEndian.PutUint32(data[12:], 36)
	binary.LittleEndian.PutUint32(data[16:], 40)
	binary.LittleEndian.PutUint32(data[24:], 100)
	copy(data[36:], "ATA\x00")
	copy(data[40:], "Samsung SSD 860 EVO 1TB\x00")
	copy(data[100:], "PRIVATE-SERIAL-MUST-NOT-BE-READ\x00")
	if name := storageDeviceModel(data); name != "Samsung SSD 860 EVO 1TB" {
		t.Fatalf("model incorrectly decoded: %q", name)
	}
	binary.LittleEndian.PutUint32(data[16:], ^uint32(0))
	if storageDeviceModel(data) != "" {
		t.Fatal("accepted invalid product offset")
	}
	binary.LittleEndian.PutUint32(data[16:], 40)
	for i := 40; i < 140; i++ {
		data[i] = 'x'
	}
	if storageDeviceModel(data) != "" {
		t.Fatal("accepted missing string terminator")
	}
	binary.LittleEndian.PutUint32(data[4:], 141)
	if storageDeviceModel(data) != "" || storageDeviceModel(data[:35]) != "" {
		t.Fatal("accepted short descriptor")
	}
}
func TestDiskHealthHardware(t *testing.T) {
	if os.Getenv("NEXUS_HARDWARE_TESTS") != "1" {
		t.Skip("physical hardware probe is opt-in: NEXUS_HARDWARE_TESTS=1")
	}
	for i := uint32(0); i < 2; i++ {
		t.Logf("disk %d bus %s", i, deviceBus(i))
		if h, err := deviceHandle(i); err == nil {
			if descriptor, err := storageQuery(h, propertyQuery(0), 1024); err == nil {
				// Product model is useful hardware evidence; no serial offset
				// or vendor-specific descriptor bytes are printed.
				t.Logf("disk %d model %s", i, storageDeviceModel(descriptor))
			}
			_ = windows.CloseHandle(h)
		}
		result := readDiskHealthDetails(i)
		t.Logf("disk %d ATA attributes=%d", i, len(result.attributes))
		for _, m := range result.metrics {
			if m.Value == nil {
				t.Logf("%s unavailable diagnostic=%+v", m.ID, m.Diagnostic)
			} else {
				t.Logf("%s %v %s source=%s", m.ID, *m.Value, m.Unit, m.Source)
			}
		}
	}
}
