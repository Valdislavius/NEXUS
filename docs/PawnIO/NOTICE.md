# Intel CPU Package / PawnIO

NEXUS calls the installed PawnIO driver directly from Go. It loads the official signed IntelMSR module and invokes only ioctl_read_msr for IA32_TEMPERATURE_TARGET (0x1A2) and IA32_PACKAGE_THERM_STATUS (0x1B1). No writes to hardware registers are implemented.

Module: PawnIO.Modules 0.2.11, LGPL-2.1, copyright namazso and contributors.
Upstream: https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11
Redistributed module downloaded from LibreHardwareMonitor commit dc51e75bd97b15ce17ded0885e67bad47be0765b.
SHA256: D6ED85D65AB17A22F813EF98207D6D537155EE2DED5976A21CB48413C9B92E5F

COPYING contains the module license. PawnIO.Modules-0.2.11-source.zip contains its corresponding source. NEXUS-collector-source.zip contains the complete Go collector source, module and dependency lock files. To rebuild/replace the module, unpack this source, replace internal/platform/pawnio/IntelMSR.bin with a compatible signed module, then run `go build -o nexus-collector.exe ./cmd/nexus-collector`. The PawnIO driver enforces its own signature policy. NEXUS imposes no additional module hash lock. Development tools/dependencies may require internet for rebuilding. Reverse engineering for debugging modifications to this LGPL component is permitted.

Protocol and temperature interpretation were checked against LibreHardwareMonitor's PawnIo.cs, IntelMsr.cs and IntelCpu.cs. The application does not include or execute LibreHardwareMonitor/.NET. LibreHardwareMonitor is used separately only for validation.

PawnIO itself is not installed automatically or bundled as a driver. Install it explicitly from https://pawnio.eu if needed. Release 0.3.1 requests administrator rights through the standard Windows UAC manifest automatically. Normal installation does not require this access. Without it, temperature is unavailable and other monitoring continues.
