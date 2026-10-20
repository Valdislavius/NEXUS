//go:build windows && amd64

package platform

// An explicit AMD adapter keeps Intel MSR entirely out of the AMD code path.
// PawnIO AMDFamily17 ioctl_read_smn performs pci_config_write_dword to select
// the SMN address. Under NEXUS's prohibition on hardware register writes this
// transport cannot be used, even though its exported command is named read.
// See docs/CPU-COMPATIBILITY-0.3.1.md for AMD OSRR and upstream evidence.
type amdCPUThermometer struct{ identity cpuIdentity }

func (*amdCPUThermometer) close()         {}
func (*amdCPUThermometer) source() string { return "AMD CPU · CPUID / SMN compatibility" }
func (*amdCPUThermometer) description() string {
	return "AMD Tctl/Tdie · датчик не поддерживается в этой версии"
}
func (t *amdCPUThermometer) temperature() (float64, error) {
	if t.identity.family >= 0x17 && t.identity.family <= 0x1a {
		return 0, newCPUSensorError("unsupported", "AMD_SMN_SELECTOR_WRITE_REQUIRED", "Температура AMD пока не поддерживается в безопасном режиме чтения", nil)
	}
	return 0, newCPUSensorError("unsupported", "AMD_FAMILY_UNVERIFIED", "Для этого семейства AMD безопасный датчик температуры не подтверждён", nil)
}
