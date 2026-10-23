//go:build windows

package platform

import (
	"fmt"
	"golang.org/x/sys/windows"
	"nexus/collector/internal/model"
	"runtime"
	"strings"
	"unsafe"
)

type processorPower struct{ Number, MaxMHz, CurrentMHz, Limit, MaxIdle, CurrentIdle uint32 }

func powerClocks() []float64 {
	if runtime.NumCPU() > 64 {
		return nil
	} // Processor groups need explicit mapping; do not guess.
	p := windows.NewLazySystemDLL("powrprof.dll").NewProc("CallNtPowerInformation")
	if p.Find() != nil {
		return nil
	}
	buf := make([]processorPower, runtime.NumCPU())
	code, _, _ := p.Call(11, 0, 0, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf))*unsafe.Sizeof(buf[0]))
	if code != 0 {
		return nil
	}
	out := make([]float64, len(buf))
	for i, r := range buf {
		if r.CurrentMHz == 0 {
			return nil
		}
		out[i] = float64(r.CurrentMHz)
	}
	return out
}

type nvml struct {
	dll     *windows.LazyDLL
	handles []uintptr
	ready   bool
	failure *model.Diagnostic
	invoke  func(string, ...uintptr) uintptr // Optional test seam; production uses the installed NVIDIA DLL.
}

// Return categories follow NVIDIA's nvmlReturn_t enum. The code is numeric and
// bounded; no driver prose, paths, device identifiers or library errors escape.
// https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceEnums.html
func nvmlDiagnostic(code uintptr) *model.Diagnostic {
	status := "transient_error"
	switch code {
	case 0:
		return &model.Diagnostic{Status: "invalid_response", Code: "NVML_VALUE_INVALID"}
	case 3, 22, 24, 26, 30:
		status = "unsupported"
	case 4, 17:
		status = "permission_denied"
	case 9, 12, 13, 18:
		status = "dependency_missing"
	case 8, 11, 15, 16:
		status = "device_unresponsive"
	case 1, 6, 21, 28:
		status = "driver_no_data"
	case 2, 7, 14, 25:
		status = "invalid_response"
	}
	if code > 30 {
		code = 999
	}
	return &model.Diagnostic{Status: status, Code: fmt.Sprintf("NVML_%d", code)}
}
func nvmlMissing(id, label, unit string, primary bool, diagnostic *model.Diagnostic) model.Metric {
	reasons := map[string]string{
		"unsupported": "Датчик не поддерживается", "permission_denied": "Недостаточно прав",
		"dependency_missing": "Отсутствует необходимая зависимость", "device_unresponsive": "Устройство не отвечает",
		"driver_no_data": "Драйвер не предоставляет данные", "invalid_response": "Некорректный ответ",
		"transient_error": "Временная ошибка чтения",
	}
	return model.MissingDiagnostic(id, label, unit, "NVIDIA NVML", reasons[diagnostic.Status], diagnostic.Status, diagnostic.Code, primary)
}
func nvmlScalarMetric(id, label, unit string, value uint32, scale float64, primary bool, code uintptr) model.Metric {
	if code != 0 {
		return nvmlMissing(id, label, unit, primary, nvmlDiagnostic(code))
	}
	// The absent-value sentinel and impossible unsigned temperature must not
	// become enormous numeric readings. Fan percentages may legitimately exceed
	// 100 according to NVIDIA's API; retain them without an invented limit.
	if value == ^uint32(0) || (id == "temperature" && value > 255) {
		return nvmlMissing(id, label, unit, primary, nvmlDiagnostic(0))
	}
	return model.Known(id, label, unit, "NVIDIA NVML", float64(value)/scale, primary)
}

func openNVML() *nvml {
	n := &nvml{dll: windows.NewLazySystemDLL("nvml.dll")}
	if n.dll.Load() != nil {
		n.failure = nvmlDiagnostic(12)
		return n
	}
	if code := n.call("nvmlInit_v2"); code != 0 {
		n.failure = nvmlDiagnostic(code)
		return n
	}
	n.ready = true
	var count uint32
	if code := n.call("nvmlDeviceGetCount_v2", uintptr(unsafe.Pointer(&count))); code != 0 {
		n.failure = nvmlDiagnostic(code)
		return n
	}
	if count > 32 {
		n.failure = nvmlDiagnostic(0)
		return n
	}
	for i := uint32(0); i < count; i++ {
		var handle uintptr
		if code := n.call("nvmlDeviceGetHandleByIndex_v2", uintptr(i), uintptr(unsafe.Pointer(&handle))); code == 0 && handle != 0 {
			n.handles = append(n.handles, handle)
		} else {
			n.failure = nvmlDiagnostic(code)
		}
	}
	return n
}
func (n *nvml) call(name string, args ...uintptr) uintptr {
	if n.invoke != nil {
		return n.invoke(name, args...)
	}
	p := n.dll.NewProc(name)
	if p.Find() != nil {
		return 13 // NVML_ERROR_FUNCTION_NOT_FOUND, distinct from a driver error.
	}
	r, _, _ := p.Call(args...)
	return r
}
func (n *nvml) close() {
	if n.ready {
		n.call("nvmlShutdown")
		n.ready = false
	}
}
func cString(buf []byte) string {
	s := string(buf)
	if i := strings.IndexByte(s, 0); i >= 0 {
		s = s[:i]
	}
	return s
}
func (n *nvml) inventory() []model.Component {
	out := []model.Component{}
	for i, h := range n.handles {
		name := make([]byte, 256)
		if code := n.call("nvmlDeviceGetName", h, uintptr(unsafe.Pointer(&name[0])), 256); code != 0 {
			n.failure = nvmlDiagnostic(code)
			continue
		}
		c := model.New(fmt.Sprintf("nv-%d", i), "gpu", cString(name), "NVIDIA · датчики NVML")
		driver := make([]byte, 128)
		if n.call("nvmlSystemGetDriverVersion", uintptr(unsafe.Pointer(&driver[0])), 128) == 0 {
			c.Spec("Драйвер NVIDIA", cString(driver))
		}
		gpuUnavailable(&c)
		c.Note = "Данные драйвера NVIDIA. Скорость вентилятора указана в процентах от номинальной скорости."
		out = append(out, c)
	}
	return out
}
func gpuUnavailable(c *model.Component) {
	why := "Датчик не предоставлен доступным API драйвера"
	for _, m := range []struct {
		id, label, unit string
		primary         bool
	}{{"load", "Загрузка", "%", true}, {"temperature", "Температура", "°C", true}, {"memoryUsed", "Видеопамять", "bytes", true}, {"memoryTotal", "Объём VRAM", "bytes", false}, {"clock", "Частота GPU", "МГц", false}, {"memoryClock", "Частота памяти", "МГц", false}, {"power", "Мощность", "Вт", false}, {"hotspot", "Hotspot", "°C", false}, {"fan", "Вентилятор", "%", false}} {
		missing(c, m.id, m.label, m.unit, why, m.primary)
	}
}
func (n *nvml) sample(c *model.Component) {
	var index int
	if _, err := fmt.Sscanf(c.ID, "nv-%d", &index); err != nil || index < 0 || index >= len(n.handles) {
		// WMI can identify an NVIDIA adapter even when NVML initialization or
		// enumeration failed. Diagnose its primary sensors without inventing
		// extra sensors that have never been queried.
		if strings.Contains(strings.ToUpper(c.Name), "NVIDIA") {
			diagnostic := n.failure
			if diagnostic == nil {
				diagnostic = nvmlDiagnostic(6)
			}
			for _, m := range c.Metrics {
				if m.Primary && (m.ID == "load" || m.ID == "temperature" || m.ID == "memoryUsed") {
					c.Set(nvmlMissing(m.ID, m.Label, m.Unit, true, diagnostic))
				}
			}
		}
		return
	}
	h := n.handles[index]
	read := func(id, label, unit, fn string, scale float64, primary bool, args ...uintptr) {
		var v uint32
		p := []uintptr{h}
		p = append(p, args...)
		p = append(p, uintptr(unsafe.Pointer(&v)))
		code := n.call(fn, p...)
		c.Set(nvmlScalarMetric(id, label, unit, v, scale, primary, code))
	}
	read("temperature", "Температура", "°C", "nvmlDeviceGetTemperature", 1, true, 0)
	read("clock", "Частота GPU", "МГц", "nvmlDeviceGetClockInfo", 1, false, 0)
	read("memoryClock", "Частота памяти", "МГц", "nvmlDeviceGetClockInfo", 1, false, 2)
	read("power", "Мощность", "Вт", "nvmlDeviceGetPowerUsage", 1000, false)
	read("fan", "Вентилятор", "%", "nvmlDeviceGetFanSpeed", 1, false)
	var u struct{ GPU, Memory uint32 }
	if code := n.call("nvmlDeviceGetUtilizationRates", h, uintptr(unsafe.Pointer(&u))); code == 0 && u.GPU <= 100 && u.Memory <= 100 {
		c.Set(model.Known("load", "Загрузка", "%", "NVIDIA NVML", float64(u.GPU), true))
	} else {
		c.Set(nvmlMissing("load", "Загрузка", "%", true, nvmlDiagnostic(code)))
	}
	var m struct{ Total, Free, Used uint64 }
	if code := n.call("nvmlDeviceGetMemoryInfo", h, uintptr(unsafe.Pointer(&m))); code == 0 && m.Total > 0 && m.Used <= m.Total && m.Free <= m.Total {
		c.Set(model.Known("memoryUsed", "Видеопамять", "bytes", "NVIDIA NVML", float64(m.Used), true))
		c.Set(model.Known("memoryTotal", "Объём VRAM", "bytes", "NVIDIA NVML", float64(m.Total), false))
	} else {
		c.Set(nvmlMissing("memoryUsed", "Видеопамять", "bytes", true, nvmlDiagnostic(code)))
		c.Set(nvmlMissing("memoryTotal", "Объём VRAM", "bytes", false, nvmlDiagnostic(code)))
	}
}
