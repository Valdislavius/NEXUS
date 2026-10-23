//go:build windows && !amd64

package platform

type cpuThermometer struct{}

func (*cpuThermometer) close()         {}
func (*cpuThermometer) source() string { return "CPU Package" }
func (*cpuThermometer) description() string {
	return "CPU Package · архитектура не поддерживается"
}
func (*cpuThermometer) temperature() (float64, error) {
	return 0, newCPUSensorError("unsupported", "CPU_ARCHITECTURE_UNSUPPORTED", "CPU Package: архитектура пока не поддерживается", nil)
}
