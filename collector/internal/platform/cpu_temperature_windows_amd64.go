//go:build windows && amd64

package platform

import (
	_ "embed"
	"encoding/binary"
	"runtime"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

// Official signed PawnIO.Modules 0.2.11 IntelMSR; see docs/PawnIO.
//
//go:embed pawnio/IntelMSR.bin
var intelMSRModule []byte

const (
	msrTemperatureTarget    = uint64(0x1a2)
	msrPackageThermalStatus = uint64(0x1b1)
	pawnIOLoadModule        = uint32(0xa1b20000 | (0x821 << 2))
	pawnIOExecuteRead       = uint32(0xa1b22000 | 0x104)
	cpuOpenRetryInterval    = 30 * time.Second
	maxCPUOpenAttempts      = 5
)

func cpuid(leaf, subleaf uint32) (a, b, c, d uint32)

type cpuidReader func(leaf, subleaf uint32) (a, b, c, d uint32)

type cpuIdentity struct {
	vendor                           string
	maxLeaf, family, model, stepping uint32
}

func identifyCPU(read cpuidReader) cpuIdentity {
	max, b, c, d := read(0, 0)
	buf := make([]byte, 12)
	binary.LittleEndian.PutUint32(buf[0:], b)
	binary.LittleEndian.PutUint32(buf[4:], d)
	binary.LittleEndian.PutUint32(buf[8:], c)
	id := cpuIdentity{vendor: string(buf), maxLeaf: max}
	if max < 1 {
		return id
	}
	signature, _, _, _ := read(1, 0)
	baseFamily := (signature >> 8) & 0xf
	id.family = baseFamily
	if baseFamily == 0xf {
		id.family += (signature >> 20) & 0xff
	}
	id.model = (signature >> 4) & 0xf
	if baseFamily == 0x6 || baseFamily == 0xf {
		id.model |= ((signature >> 16) & 0xf) << 4
	}
	id.stepping = signature & 0xf
	return id
}

type cpuTemperatureAdapter interface {
	temperature() (float64, error)
	close()
	source() string
	description() string
}

// Adapter selection precedes all privileged access. Hardware redetection closes
// the old adapter; AMD can never reach IntelMSR through this dispatcher.
type cpuThermometer struct {
	adapter    cpuTemperatureAdapter
	identity   cpuIdentity
	readCPU    cpuidReader
	newAdapter func(cpuIdentity, cpuidReader) cpuTemperatureAdapter
}

func (t *cpuThermometer) detect() cpuTemperatureAdapter {
	read := t.readCPU
	if read == nil {
		read = cpuid
	}
	id := identifyCPU(read)
	if t.adapter == nil || t.identity != id {
		t.close()
		t.identity = id
		if t.newAdapter != nil {
			t.adapter = t.newAdapter(id, read)
		} else {
			switch id.vendor {
			case "GenuineIntel":
				t.adapter = &intelCPUThermometer{readCPU: read}
			case "AuthenticAMD":
				t.adapter = &amdCPUThermometer{identity: id}
			default:
				t.adapter = unsupportedCPUThermometer{}
			}
		}
	}
	return t.adapter
}

func (t *cpuThermometer) close() {
	if t.adapter != nil {
		t.adapter.close()
		t.adapter = nil
	}
}
func (t *cpuThermometer) temperature() (float64, error) { return t.detect().temperature() }
func (t *cpuThermometer) source() string                { return t.detect().source() }
func (t *cpuThermometer) description() string           { return t.detect().description() }

type unsupportedCPUThermometer struct{}

func (unsupportedCPUThermometer) close()         {}
func (unsupportedCPUThermometer) source() string { return "CPU Package" }
func (unsupportedCPUThermometer) description() string {
	return "Для этого производителя CPU безопасный источник температуры не подтверждён."
}
func (unsupportedCPUThermometer) temperature() (float64, error) {
	return 0, newCPUSensorError("unsupported", "CPU_VENDOR_UNSUPPORTED", "Датчик температуры этого процессора не поддерживается", nil)
}

// The interface exposes only the two allowlisted reads. There is no MSR write,
// arbitrary IOCTL, PCI, SMU, SMN or register-address input from IPC.
type intelTemperatureDriver interface {
	open() (windows.Handle, error)
	load(windows.Handle) error
	read(windows.Handle, uint64) (uint64, error)
	close(windows.Handle)
}
type nativeIntelTemperatureDriver struct{}

func (nativeIntelTemperatureDriver) open() (windows.Handle, error) {
	name, _ := windows.UTF16PtrFromString(`\\?\GLOBALROOT\Device\PawnIO`)
	return windows.CreateFile(name, 3, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, nil, windows.OPEN_EXISTING, windows.FILE_ATTRIBUTE_NORMAL, 0)
}
func (nativeIntelTemperatureDriver) load(h windows.Handle) error {
	var received uint32
	return windows.DeviceIoControl(h, pawnIOLoadModule, &intelMSRModule[0], uint32(len(intelMSRModule)), nil, 0, &received, nil)
}
func (nativeIntelTemperatureDriver) close(h windows.Handle) { windows.CloseHandle(h) }
func (nativeIntelTemperatureDriver) read(h windows.Handle, index uint64) (uint64, error) {
	if index != msrTemperatureTarget && index != msrPackageThermalStatus {
		return 0, newCPUSensorError("unsupported", "CPU_MSR_NOT_PERMITTED", "Запрошенный регистр CPU не разрешён", nil)
	}
	input := make([]byte, 40)
	copy(input, "ioctl_read_msr")
	binary.LittleEndian.PutUint64(input[32:], index)
	output := make([]byte, 8)
	var received uint32
	err := windows.DeviceIoControl(h, pawnIOExecuteRead, &input[0], uint32(len(input)), &output[0], 8, &received, nil)
	if err != nil {
		return 0, err
	}
	return decodeMSRResponse(output, received)
}

func decodeMSRResponse(output []byte, received uint32) (uint64, error) {
	if received != 8 || len(output) != 8 {
		return 0, newCPUSensorError("invalid_response", "PAWNIO_MSR_RESPONSE_LENGTH", "Драйвер CPU вернул неполный ответ", nil)
	}
	return binary.LittleEndian.Uint64(output), nil
}

type intelCPUThermometer struct {
	handle       windows.Handle
	driver       intelTemperatureDriver
	readCPU      cpuidReader
	bindThread   func() (func(), error)
	now          func() time.Time
	nextOpen     time.Time
	lastError    error
	openAttempts int
}

func (t *intelCPUThermometer) init() {
	if t.driver == nil {
		t.driver = nativeIntelTemperatureDriver{}
	}
	if t.readCPU == nil {
		t.readCPU = cpuid
	}
	if t.bindThread == nil {
		t.bindThread = bindCPUTemperatureThread
	}
	if t.now == nil {
		t.now = time.Now
	}
}
func (t *intelCPUThermometer) close() {
	if t.handle != 0 {
		t.driver.close(t.handle)
		t.handle = 0
	}
}
func (*intelCPUThermometer) source() string      { return "Intel CPU Package · PawnIO / MSR 0x1B1" }
func (*intelCPUThermometer) description() string { return "CPU Package · Intel MSR / PawnIO" }
func (t *intelCPUThermometer) fail(err error) (float64, error) {
	t.close()
	t.lastError = err
	t.nextOpen = t.now().Add(cpuOpenRetryInterval)
	return 0, err
}

func decodePackageTemperature(target, status uint64) (float64, error) {
	tj := int((target >> 16) & 255)
	delta := int((status >> 16) & 127)
	if tj < 50 || tj > 125 || status&(1<<31) == 0 || delta > tj {
		return 0, newCPUSensorError("invalid_response", "INTEL_PACKAGE_INVALID", "CPU Package: недостоверный ответ датчика", nil)
	}
	return float64(tj - delta), nil
}

func (t *intelCPUThermometer) temperature() (float64, error) {
	t.init()
	// Failed devices have a finite reopen budget until explicit rediscovery.
	if t.handle == 0 && t.openAttempts >= maxCPUOpenAttempts && t.lastError != nil {
		return 0, t.lastError
	}
	if t.handle == 0 && t.now().Before(t.nextOpen) && t.lastError != nil {
		return 0, t.lastError
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	restore, err := t.bindThread()
	if err != nil {
		return t.fail(err)
	}
	defer restore()
	id := identifyCPU(t.readCPU)
	if id.vendor != "GenuineIntel" || id.maxLeaf < 6 {
		return t.fail(newCPUSensorError("unsupported", "INTEL_CPUID_UNSUPPORTED", "CPU Package: этот процессор не поддерживается", nil))
	}
	features, _, _, _ := t.readCPU(6, 0)
	if features&(1<<6) == 0 {
		return t.fail(newCPUSensorError("unsupported", "INTEL_PACKAGE_SENSOR_ABSENT", "CPU Package: пакетный датчик не поддерживается", nil))
	}
	if t.handle == 0 {
		t.openAttempts++
		h, err := t.driver.open()
		if err != nil {
			return t.fail(classifyCPUDriverError("open", err))
		}
		if err = t.driver.load(h); err != nil {
			t.driver.close(h)
			return t.fail(classifyCPUDriverError("load", err))
		}
		t.handle = h
	}
	target, err := t.driver.read(t.handle, msrTemperatureTarget)
	if err != nil {
		return t.fail(classifyCPUDriverError("read_target", err))
	}
	status, err := t.driver.read(t.handle, msrPackageThermalStatus)
	if err != nil {
		return t.fail(classifyCPUDriverError("read_status", err))
	}
	value, err := decodePackageTemperature(target, status)
	// A complete but invalid sample is not a handle failure. Clear its value and
	// retry only the read on the next sample, without reopening the device.
	if err != nil {
		return 0, err
	}
	t.lastError = nil
	t.openAttempts = 0
	t.nextOpen = time.Time{}
	return value, nil
}

func bindCPUTemperatureThread() (func(), error) {
	if runtime.NumCPU() > 64 {
		return nil, newCPUSensorError("unsupported", "CPU_PROCESSOR_GROUPS_UNSUPPORTED", "CPU Package: группы более 64 процессоров пока не поддерживаются", nil)
	}
	kernel := windows.NewLazySystemDLL("kernel32.dll")
	var processMask, systemMask uintptr
	ok, _, affinityErr := kernel.NewProc("GetProcessAffinityMask").Call(uintptr(windows.CurrentProcess()), uintptr(unsafe.Pointer(&processMask)), uintptr(unsafe.Pointer(&systemMask)))
	if ok == 0 || processMask == 0 {
		return nil, newCPUSensorError("transient_error", "CPU_AFFINITY_QUERY_FAILED", "CPU Package: недоступна привязка потока", affinityErr)
	}
	setAffinity := kernel.NewProc("SetThreadAffinityMask")
	previousMask, _, affinityErr := setAffinity.Call(uintptr(windows.CurrentThread()), processMask&-processMask)
	if previousMask == 0 {
		return nil, newCPUSensorError("transient_error", "CPU_AFFINITY_SET_FAILED", "CPU Package: не удалось выбрать процессор", affinityErr)
	}
	return func() { setAffinity.Call(uintptr(windows.CurrentThread()), previousMask) }, nil
}
