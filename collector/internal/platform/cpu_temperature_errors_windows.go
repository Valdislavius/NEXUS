//go:build windows

package platform

import (
	"errors"
	"fmt"
	"syscall"

	"golang.org/x/sys/windows"
	"nexus/collector/internal/model"
)

type cpuSensorError struct {
	status, code, reason string
	cause                error
}

func (e *cpuSensorError) Error() string { return e.reason }
func (e *cpuSensorError) Unwrap() error { return e.cause }
func newCPUSensorError(status, code, reason string, cause error) error {
	return &cpuSensorError{status: status, code: code, reason: reason, cause: cause}
}
func cpuTemperatureDiagnostic(err error) (status, code string) {
	var sensor *cpuSensorError
	if errors.As(err, &sensor) {
		return sensor.status, sensor.code
	}
	return "transient_error", "CPU_READ_FAILED"
}

func sampleCPUTemperature(t *cpuThermometer, singleCPU bool) model.Metric {
	if !singleCPU {
		t.close()
		return model.MissingDiagnostic("temperature", "Температура", "°C", "CPU Package", "CPU Package: требуется один физический процессор", "unsupported", "CPU_MULTIPLE_PACKAGES_UNSUPPORTED", true)
	}
	value, err := t.temperature()
	if err == nil {
		return model.Known("temperature", "Температура", "°C", t.source(), value, true)
	}
	status, code := cpuTemperatureDiagnostic(err)
	return model.MissingDiagnostic("temperature", "Температура", "°C", t.source(), err.Error(), status, code, true)
}

func classifyCPUDriverError(stage string, err error) error {
	var sensor *cpuSensorError
	if errors.As(err, &sensor) {
		return err
	}
	status, reason := "transient_error", "Временная ошибка чтения температуры CPU"
	switch {
	case errors.Is(err, windows.ERROR_ACCESS_DENIED), errors.Is(err, windows.ERROR_PRIVILEGE_NOT_HELD):
		status, reason = "permission_denied", "Недостаточно прав для доступа к датчику CPU через PawnIO"
	case stage == "open" && (errors.Is(err, windows.ERROR_FILE_NOT_FOUND) || errors.Is(err, windows.ERROR_PATH_NOT_FOUND)):
		status, reason = "dependency_missing", "Для температуры Intel нужен установленный PawnIO"
	case errors.Is(err, windows.ERROR_NOT_READY), errors.Is(err, windows.ERROR_DEVICE_NOT_CONNECTED), errors.Is(err, windows.ERROR_SEM_TIMEOUT), errors.Is(err, windows.ERROR_TIMEOUT):
		status, reason = "device_unresponsive", "Устройство датчика CPU не отвечает"
	case errors.Is(err, windows.ERROR_NOT_SUPPORTED), errors.Is(err, windows.ERROR_INVALID_FUNCTION):
		status, reason = "driver_no_data", "Драйвер CPU не предоставляет данные этого датчика"
	case errors.Is(err, windows.ERROR_INSUFFICIENT_BUFFER), errors.Is(err, windows.ERROR_INVALID_DATA):
		status, reason = "invalid_response", "Драйвер CPU вернул некорректный ответ"
	case stage == "load":
		status, reason = "driver_no_data", "PawnIO не загрузил подписанный модуль IntelMSR"
	}
	code := "CPU_READ_FAILED"
	switch stage {
	case "open":
		code = "PAWNIO_OPEN_FAILED"
	case "load":
		code = "PAWNIO_LOAD_FAILED"
	case "read_target":
		code = "PAWNIO_READ_TARGET_FAILED"
	case "read_status":
		code = "PAWNIO_READ_STATUS_FAILED"
	}
	var errno syscall.Errno
	if errors.As(err, &errno) {
		code = fmt.Sprintf("WIN32_%d", uint32(errno))
	}
	return newCPUSensorError(status, code, reason, err)
}
