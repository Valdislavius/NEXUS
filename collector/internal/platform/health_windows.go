//go:build windows

package platform

import (
	"encoding/binary"
	"fmt"
	"golang.org/x/sys/windows"
	"log"
	"math"
	"nexus/collector/internal/model"
	"strings"
	"sync"
)

const storageQueryProperty = 0x002d1400

func storageQuery(h windows.Handle, input []byte, size int) ([]byte, error) {
	if len(input) == 0 || size <= 0 {
		return nil, fmt.Errorf("invalid storage query")
	}
	out := make([]byte, size)
	var returned uint32
	err := windows.DeviceIoControl(h, storageQueryProperty, &input[0], uint32(len(input)), &out[0], uint32(len(out)), &returned, nil)
	if err != nil {
		return nil, err
	}
	if returned > uint32(len(out)) {
		return nil, fmt.Errorf("invalid storage response")
	}
	return out[:returned], nil
}
func propertyQuery(id uint32) []byte {
	q := make([]byte, 12)
	binary.LittleEndian.PutUint32(q, id)
	return q
}
func deviceHandle(index uint32) (windows.Handle, error) {
	path, err := windows.UTF16PtrFromString(fmt.Sprintf(`\\.\PhysicalDrive%d`, index))
	if err != nil {
		return 0, err
	}
	return windows.CreateFile(path, 0, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, nil, windows.OPEN_EXISTING, 0, 0)
}
func deviceBus(index uint32) string {
	h, err := deviceHandle(index)
	if err != nil {
		return ""
	}
	defer windows.CloseHandle(h)
	return deviceBusFromHandle(h)
}
func deviceBusFromHandle(h windows.Handle) string {
	data, err := storageQuery(h, propertyQuery(0), 1024)
	if err != nil || len(data) < 36 {
		return ""
	}
	buses := map[uint32]string{1: "SCSI", 3: "ATA", 7: "USB", 8: "RAID", 11: "SATA", 12: "SD", 14: "Виртуальный", 17: "NVMe", 18: "SCM", 19: "UFS"}
	return buses[binary.LittleEndian.Uint32(data[28:32])]
}

// Decode only vendor/product offsets. SerialNumberOffset is deliberately ignored.
func storageDeviceModel(data []byte) string {
	if len(data) < 36 {
		return ""
	}
	size := uint64(binary.LittleEndian.Uint32(data[4:8]))
	if size < 36 || size > uint64(len(data)) {
		return ""
	}
	readASCII := func(field int) string {
		offset := uint64(binary.LittleEndian.Uint32(data[field : field+4]))
		if offset < 36 || offset >= size {
			return ""
		}
		for end := offset; end < size && end-offset <= 80; end++ {
			if data[end] == 0 {
				return strings.TrimSpace(string(data[offset:end]))
			}
			if data[end] < 32 || data[end] > 126 {
				return ""
			}
		}
		return ""
	}
	vendor, product := readASCII(12), readASCII(16)
	if strings.EqualFold(vendor, "ATA") || strings.HasPrefix(strings.ToUpper(product), strings.ToUpper(vendor)+" ") {
		vendor = ""
	}
	return strings.TrimSpace(vendor + " " + product)
}
func storageTemperature(data []byte) *float64 {
	if len(data) < 40 || binary.LittleEndian.Uint16(data[12:14]) == 0 {
		return nil
	}
	// The first descriptor must be the composite sensor (Index = 0).
	if binary.LittleEndian.Uint16(data[24:26]) != 0 {
		return nil
	}
	v := int16(binary.LittleEndian.Uint16(data[26:28]))
	if v == -32768 || v <= 0 || v > 150 {
		return nil
	}
	f := float64(v)
	return &f
}
func le128(data []byte) float64 {
	return float64(binary.LittleEndian.Uint64(data[:8])) + math.Ldexp(float64(binary.LittleEndian.Uint64(data[8:16])), 64)
}

// NVMe SMART/Health Information log, NVMe spec section 5.14.1.2. Values
// are decoded only after validating descriptor offsets and the full 512-byte log.
func parseNVMeHealth(data []byte) []model.Metric {
	if len(data) < 48 {
		return nil
	}
	if binary.LittleEndian.Uint32(data[8:12]) != 3 || binary.LittleEndian.Uint32(data[12:16]) != 2 {
		return nil
	}
	offset := uint64(binary.LittleEndian.Uint32(data[24:28])) + 8
	length := uint64(binary.LittleEndian.Uint32(data[28:32]))
	if offset < 48 || length < 512 || offset+length > uint64(len(data)) {
		return nil
	}
	log := data[offset : offset+512]
	result := []model.Metric{}
	add := func(id, label, unit string, v float64, primary bool) {
		result = append(result, model.Known(id, label, unit, "NVMe SMART", v, primary))
	}
	kelvin := binary.LittleEndian.Uint16(log[1:3])
	if kelvin > 273 && kelvin < 424 {
		add("temperature", "Температура", "°C", float64(kelvin)-273.15, true)
	}
	add("smart-warning", "Критические предупреждения", "", float64(log[0]), false)
	add("spare", "Резерв", "%", float64(log[3]), false)
	add("spare-threshold", "Порог резерва", "%", float64(log[4]), false)
	add("wear", "Ресурс использован", "%", float64(log[5]), false)
	add("lifetime-read", "Прочитано за всё время", "bytes", le128(log[32:48])*512000, false)
	add("lifetime-written", "Записано за всё время", "bytes", le128(log[48:64])*512000, false)
	add("power-cycles", "Включения", "", le128(log[112:128]), false)
	add("power-hours", "Наработка", "hours", le128(log[128:144]), false)
	add("unsafe-shutdowns", "Небезопасные отключения", "", le128(log[144:160]), false)
	add("media-errors", "Ошибки целостности данных", "", le128(log[160:176]), false)
	return result
}

type diskHealthResult struct {
	metrics    []model.Metric
	attributes []model.SMARTAttribute
}

func readDiskHealth(index uint32) []model.Metric { return readDiskHealthDetails(index).metrics }

func readDiskHealthDetails(index uint32) diskHealthResult {
	h, err := deviceHandle(index)
	if err != nil {
		return diskHealthResult{metrics: []model.Metric{ataMissing("smart-data", "Диагностика SMART", err)}}
	}
	defer windows.CloseHandle(h)
	bus := deviceBusFromHandle(h)
	q := make([]byte, 48)
	binary.LittleEndian.PutUint32(q, 49)
	binary.LittleEndian.PutUint32(q[8:], 3)
	binary.LittleEndian.PutUint32(q[12:], 2)
	binary.LittleEndian.PutUint32(q[16:], 2)
	binary.LittleEndian.PutUint32(q[24:], 40)
	binary.LittleEndian.PutUint32(q[28:], 512)
	if data, err := storageQuery(h, q, 560); err == nil {
		if metrics := parseNVMeHealth(data); len(metrics) > 0 {
			return diskHealthResult{metrics: metrics}
		}
	}
	result := []model.Metric{}
	var attributes []model.SMARTAttribute
	if bus == "ATA" || bus == "SATA" {
		ataResult, ataErr := readATAHealth(index, nativeATADeviceIO{})
		if ataErr != nil {
			result = append(result, ataMissing("smart-data", "Диагностика SMART", ataErr))
		} else {
			result = append(result, ataResult.metrics...)
			attributes = ataResult.attributes
		}
	} else if bus != "NVMe" {
		result = append(result, model.MissingDiagnostic("smart-data", "Диагностика SMART", "", "ATA SMART", "ATA SMART недоступен через этот интерфейс или контроллер", "unsupported", "ATA_BUS_UNSUPPORTED", false))
	} else {
		result = append(result, model.MissingDiagnostic("smart-data", "Диагностика SMART", "", "NVMe SMART", "Драйвер не предоставил NVMe SMART", "driver_no_data", "NVME_INVALID_RESPONSE", false))
	}
	prediction := make([]byte, 516)
	var received uint32
	if err := windows.DeviceIoControl(h, 0x002d1100, nil, 0, &prediction[0], uint32(len(prediction)), &received, nil); err == nil && received >= 516 {
		value := binary.LittleEndian.Uint32(prediction[:4])
		result = append(result, model.Known("smart-failure", "SMART: прогноз сбоя", "", "Windows Storage", float64(value), false))
		// Windows already provides vendor SMART data with this existing read.
		// Use it only on native ATA/SATA and after the same strict sector/table
		// validation. It supplies no thresholds or rotational identity.
		if len(attributes) == 0 && (bus == "ATA" || bus == "SATA") {
			deviceName := ""
			if descriptor, queryErr := storageQuery(h, propertyQuery(0), 1024); queryErr == nil {
				deviceName = storageDeviceModel(descriptor)
			}
			if fallback, parseErr := parseATASMART(prediction[4:], nil, deviceName, false); parseErr == nil {
				for i := len(result) - 1; i >= 0; i-- {
					if result[i].ID == "smart-data" && result[i].Source == "ATA SMART" {
						result = append(result[:i], result[i+1:]...)
					}
				}
				for i := range fallback.metrics {
					fallback.metrics[i].Description += " Источник: SMART данные Windows Storage."
				}
				result = append(result, fallback.metrics...)
				attributes = fallback.attributes
			}
		}
	}
	for _, property := range []uint32{52, 51} {
		if data, err := storageQuery(h, propertyQuery(property), 1024); err == nil {
			if temp := storageTemperature(data); temp != nil {
				// The existing Windows composite temperature has precedence;
				// ATA temperature remains available in the RAW attribute table.
				for i := len(result) - 1; i >= 0; i-- {
					if result[i].ID == "temperature" {
						result = append(result[:i], result[i+1:]...)
					}
				}
				result = append(result, model.Known("temperature", "Температура", "°C", "Windows Storage", *temp, true))
				return diskHealthResult{metrics: result, attributes: attributes}
			}
		}
	}
	return diskHealthResult{metrics: result, attributes: attributes}
}

var diskDiagnosticLog = struct {
	sync.Mutex
	states map[uint32]string
}{states: map[uint32]string{}}

// A refresh discards all previous health samples before applying the new reply.
// The table cannot remain visible after access denial, disconnect or corruption.
func applyDiskHealth(c *model.Component, index uint32) {
	applyDiskHealthResult(c, readDiskHealthDetails(index))
	state := ""
	for _, m := range c.Metrics {
		if (m.Source == "ATA SMART" || m.Source == "NVMe SMART" || m.Source == "Windows Storage") && m.Diagnostic != nil {
			state += m.Diagnostic.Status + "/" + m.Diagnostic.Code + " "
		}
	}
	diskDiagnosticLog.Lock()
	changed := diskDiagnosticLog.states[index] != state
	diskDiagnosticLog.states[index] = state
	diskDiagnosticLog.Unlock()
	if changed && state != "" {
		// stderr only, with a bounded category/code; no model, serial or path.
		log.Printf("sensor=disk-health disk=%d diagnostic=%s", index, state)
	}
}
func applyDiskHealthResult(c *model.Component, result diskHealthResult) {
	c.SMART = result.attributes
	metrics := make([]model.Metric, 0, len(c.Metrics)+len(result.metrics))
	for _, m := range c.Metrics {
		if m.Source == "NVMe SMART" || m.Source == "Windows Storage" || m.Source == "ATA SMART" {
			// Availability rows are rebuilt for this refresh, so successful
			// recovery does not keep a stale failure alongside valid data.
			if m.ID == "smart-data" || m.ID == "smart-thresholds" {
				continue
			}
			failure := model.MissingDiagnostic(m.ID, m.Label, m.Unit, m.Source, "Показатель отсутствует в текущем ответе накопителя", "driver_no_data", "DISK_NO_CURRENT_DATA", m.Primary)
			for _, current := range result.metrics {
				if current.Source == m.Source && current.ID == "smart-data" && current.Diagnostic != nil {
					failure.Reason, failure.Diagnostic = current.Reason, current.Diagnostic
					break
				}
			}
			metrics = append(metrics, failure)
		} else {
			metrics = append(metrics, m)
		}
	}
	c.Metrics = metrics
	for _, m := range result.metrics {
		c.Set(m)
	}
}
