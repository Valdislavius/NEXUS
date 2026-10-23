//go:build windows

package platform

import (
	"encoding/binary"
	"errors"
	"fmt"
	"nexus/collector/internal/model"
	"regexp"
	"strconv"
	"strings"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	ioctlATAPassThrough = 0x0004d02c
	ataIdentify         = 0xec
	ataSMART            = 0xb0
	ataSMARTReadData    = 0xd0
	ataSMARTThresholds  = 0xd1
	ataSectorSize       = 512
)

// ATA_PASS_THROUGH_EX uses ULONG_PTR, so its native size is 48 bytes on
// Windows amd64/arm64 and 40 on Windows 386. The packet keeps the data aligned.
type ataPassThroughHeader struct {
	Length, Flags           uint16
	Path, Target, LUN, Pad  uint8
	Transfer, Timeout, Pad2 uint32
	DataOffset              uintptr
	Previous, Current       [8]byte
}
type ataReadPacket struct {
	Header ataPassThroughHeader
	Data   [ataSectorSize]byte
}

type ataSensorError struct{ status, code, reason string }

func (e *ataSensorError) Error() string { return e.code }
func ataInvalid(code string) error {
	return &ataSensorError{"invalid_response", code, "Накопитель вернул некорректные данные SMART"}
}
func ataErrorDiagnostic(err error) *ataSensorError {
	var sensor *ataSensorError
	if errors.As(err, &sensor) {
		return sensor
	}
	var win syscall.Errno
	if errors.As(err, &win) {
		status, reason := "transient_error", "Временная ошибка чтения SMART"
		switch win {
		case windows.ERROR_ACCESS_DENIED, windows.ERROR_PRIVILEGE_NOT_HELD:
			status, reason = "permission_denied", "Недостаточно прав для чтения SMART"
		case windows.ERROR_INVALID_FUNCTION, windows.ERROR_NOT_SUPPORTED, windows.ERROR_INVALID_PARAMETER:
			status, reason = "driver_no_data", "Контроллер не предоставляет ATA SMART"
		case windows.ERROR_NOT_READY, windows.ERROR_DEV_NOT_EXIST, windows.ERROR_FILE_NOT_FOUND,
			windows.ERROR_SEM_TIMEOUT, windows.ERROR_TIMEOUT, windows.ERROR_IO_DEVICE:
			status, reason = "device_unresponsive", "Накопитель не отвечает на запрос SMART"
		}
		return &ataSensorError{status, fmt.Sprintf("WIN32_%d", uint32(win)), reason}
	}
	return &ataSensorError{"transient_error", "ATA_READ_FAILED", "Временная ошибка чтения SMART"}
}

// The closed command list is deliberate. Read/write handle access is required
// by the Windows IOCTL; no write command or arbitrary ATA taskfile is exposed.
func newATAReadPacket(command, feature byte) (ataReadPacket, error) {
	var p ataReadPacket
	if !((command == ataIdentify && feature == 0) || (command == ataSMART && (feature == ataSMARTReadData || feature == ataSMARTThresholds))) {
		return p, &ataSensorError{"unsupported", "ATA_COMMAND_NOT_ALLOWED", "Команда не разрешена"}
	}
	p.Header.Length = uint16(unsafe.Sizeof(p.Header))
	p.Header.Flags = 0x02 // ATA_FLAGS_DATA_IN; never DATA_OUT or DMA.
	p.Header.Transfer = ataSectorSize
	p.Header.Timeout = 3
	p.Header.DataOffset = unsafe.Offsetof(p.Data)
	p.Header.Current[6] = command
	if command == ataSMART {
		p.Header.Current[0] = feature
		p.Header.Current[1] = 1
		p.Header.Current[3] = 0x4f
		p.Header.Current[4] = 0xc2
	}
	return p, nil
}
func validateATAReply(p *ataReadPacket, received uint32) ([]byte, error) {
	if received > uint32(unsafe.Sizeof(*p)) || p.Header.Length != uint16(unsafe.Sizeof(p.Header)) || p.Header.DataOffset != unsafe.Offsetof(p.Data) {
		return nil, ataInvalid("ATA_INVALID_LAYOUT")
	}
	if received < uint32(unsafe.Sizeof(p.Header)) {
		return nil, ataInvalid("ATA_SHORT_RESPONSE")
	}
	status, errorRegister := p.Header.Current[6], p.Header.Current[0]
	if status&0x21 != 0 { // ERR / device fault, including ABRT.
		kind := "transient_error"
		if errorRegister&0x04 != 0 {
			kind = "driver_no_data"
		}
		return nil, &ataSensorError{kind, fmt.Sprintf("ATA_STATUS_%02X_ERROR_%02X", status, errorRegister), "Накопитель отклонил запрос ATA SMART"}
	}
	if status&0x88 != 0 || status&0x40 == 0 { // BUSY, DRQ or missing DRDY.
		return nil, ataInvalid("ATA_INVALID_STATUS")
	}
	if received < uint32(p.Header.DataOffset)+ataSectorSize || p.Header.Transfer != ataSectorSize {
		return nil, ataInvalid("ATA_SHORT_RESPONSE")
	}
	return append([]byte(nil), p.Data[:]...), nil
}
func nativeATARead(h windows.Handle, command, feature byte) ([]byte, error) {
	p, err := newATAReadPacket(command, feature)
	if err != nil {
		return nil, err
	}
	var received uint32
	size := uint32(unsafe.Sizeof(p))
	err = windows.DeviceIoControl(h, ioctlATAPassThrough, (*byte)(unsafe.Pointer(&p)), size, (*byte)(unsafe.Pointer(&p)), size, &received, nil)
	if err != nil {
		return nil, err
	}
	return validateATAReply(&p, received)
}

type ataDeviceIO interface {
	open(uint32) (windows.Handle, error)
	close(windows.Handle)
	read(windows.Handle, byte, byte) ([]byte, error)
}
type nativeATADeviceIO struct{}

func (nativeATADeviceIO) open(index uint32) (windows.Handle, error) {
	path, err := windows.UTF16PtrFromString(fmt.Sprintf(`\\.\PhysicalDrive%d`, index))
	if err != nil {
		return 0, err
	}
	return windows.CreateFile(path, windows.GENERIC_READ|windows.GENERIC_WRITE, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, nil, windows.OPEN_EXISTING, 0, 0)
}
func (nativeATADeviceIO) close(h windows.Handle) { _ = windows.CloseHandle(h) }
func (nativeATADeviceIO) read(h windows.Handle, command, feature byte) ([]byte, error) {
	return nativeATARead(h, command, feature)
}

func ataIdentity(data []byte) (name string, rotational bool, err error) {
	if len(data) != ataSectorSize || binary.LittleEndian.Uint16(data[:2]) == 0 || binary.LittleEndian.Uint16(data[:2]) == 0xffff {
		return "", false, ataInvalid("ATA_IDENTIFY_INVALID")
	}
	// Word 255 optionally supplies an integrity signature (A5h) and checksum.
	if data[510] == 0xa5 && !ataChecksum(data) {
		return "", false, ataInvalid("ATA_IDENTIFY_INVALID")
	}
	if binary.LittleEndian.Uint16(data[164:166])&1 == 0 {
		return "", false, &ataSensorError{"unsupported", "ATA_SMART_UNSUPPORTED", "Накопитель не поддерживает ATA SMART"}
	}
	if binary.LittleEndian.Uint16(data[170:172])&1 == 0 {
		return "", false, &ataSensorError{"driver_no_data", "ATA_SMART_DISABLED", "SMART отключён на накопителе"}
	}
	// Read only the model words (27-46). Serial words 10-19 are never decoded,
	// retained or logged.
	text := make([]byte, 40)
	for i := 0; i < len(text); i += 2 {
		text[i], text[i+1] = data[54+i+1], data[54+i]
	}
	for _, c := range text {
		if c != 0 && (c < 32 || c > 126) {
			return "", false, ataInvalid("ATA_IDENTIFY_INVALID")
		}
	}
	name = strings.Trim(string(text), " \x00")
	if name == "" {
		return "", false, ataInvalid("ATA_IDENTIFY_INVALID")
	}
	rpm := binary.LittleEndian.Uint16(data[434:436])
	return name, rpm > 1 && rpm < 0xffff, nil
}
func ataChecksum(data []byte) bool {
	var sum byte
	for _, v := range data {
		sum += v
	}
	return sum == 0
}
func validateSMARTSector(data []byte) error {
	if len(data) != ataSectorSize {
		return ataInvalid("ATA_SHORT_RESPONSE")
	}
	if !ataChecksum(data) {
		return ataInvalid("ATA_SMART_CHECKSUM")
	}
	revision := binary.LittleEndian.Uint16(data[:2])
	if revision == 0 || revision == 0xffff {
		return ataInvalid("ATA_SMART_REVISION")
	}
	return nil
}
func readATAHealth(index uint32, deviceIO ataDeviceIO) (diskHealthResult, error) {
	h, err := deviceIO.open(index)
	if err != nil {
		return diskHealthResult{}, err
	}
	defer deviceIO.close(h)
	identify, err := deviceIO.read(h, ataIdentify, 0)
	if err != nil {
		return diskHealthResult{}, err
	}
	name, rotational, err := ataIdentity(identify)
	if err != nil {
		return diskHealthResult{}, err
	}
	data, err := deviceIO.read(h, ataSMART, ataSMARTReadData)
	if err != nil {
		return diskHealthResult{}, err
	}
	if err = validateSMARTSector(data); err != nil {
		return diskHealthResult{}, err
	}
	// D1 is an optional legacy read. Its failure must not discard valid D0
	// attributes; absent/bad thresholds remain null, never assumed zero.
	thresholds, thresholdErr := deviceIO.read(h, ataSMART, ataSMARTThresholds)
	result, err := parseATASMART(data, thresholds, name, rotational)
	if err == nil && thresholdErr != nil {
		result.metrics = append(result.metrics, ataMissing("smart-thresholds", "Пороги SMART", thresholdErr))
	}
	return result, err
}
func ataMissing(id, label string, err error) model.Metric {
	d := ataErrorDiagnostic(err)
	return model.MissingDiagnostic(id, label, "", "ATA SMART", d.reason, d.status, d.code, false)
}
func smartThresholds(data []byte) (map[byte]uint8, error) {
	if err := validateSMARTSector(data); err != nil {
		return nil, err
	}
	values := map[byte]uint8{}
	for offset := 2; offset < 362; offset += 12 {
		id := data[offset]
		if id == 0 {
			continue
		}
		if _, seen := values[id]; seen {
			return nil, ataInvalid("ATA_SMART_DUPLICATE_ID")
		}
		values[id] = data[offset+1]
	}
	return values, nil
}

var (
	samsungSATAModel  = regexp.MustCompile(`^SAMSUNG SSD (830|840|850|860|870) (EVO|PRO|QVO)( (MSATA|M\.2))? [0-9]+(GB|TB)$`)
	crucialSATAModel  = regexp.MustCompile(`^(CRUCIAL_)?CT[0-9]+(MX100|MX200|MX300|M550)SSD[1346]$`)
	kingstonSATAModel = regexp.MustCompile(`^KINGSTON (SA400(S37|M8)|SQ500S37|SKC600(MS)?|SUV500(S37|MS|M8)?)[0-9]+G$`)
)

// Unit semantics are selected by a narrow model profile, never by an attribute
// ID alone. Unknown models still expose the exact six RAW bytes and independent
// normalized fields. See docs/SATA-SMART-0.3.1.md for source scope/limitations.
func parseATASMART(data, thresholds []byte, deviceModel string, rotational bool) (diskHealthResult, error) {
	var result diskHealthResult
	if err := validateSMARTSector(data); err != nil {
		return result, err
	}
	limits, limitErr := smartThresholds(thresholds)
	if thresholds != nil && limitErr != nil {
		result.metrics = append(result.metrics, ataMissing("smart-thresholds", "Пороги SMART", limitErr))
	}
	name := strings.ToUpper(strings.Join(strings.Fields(deviceModel), " "))
	samsung, crucial, kingston := samsungSATAModel.MatchString(name), crucialSATAModel.MatchString(name), kingstonSATAModel.MatchString(name)
	wdHDD := rotational && strings.HasPrefix(name, "WDC WD")
	seen := map[byte]bool{}
	for offset := 2; offset < 362; offset += 12 {
		entry := data[offset : offset+12]
		id := entry[0]
		if id == 0 {
			continue
		}
		if seen[id] {
			return diskHealthResult{}, ataInvalid("ATA_SMART_DUPLICATE_ID")
		}
		seen[id] = true
		var raw uint64
		for i, b := range entry[5:11] {
			raw |= uint64(b) << (8 * i)
		}
		attribute := model.SMARTAttribute{ID: id, Name: fmt.Sprintf("Атрибут %02X", id), Flags: binary.LittleEndian.Uint16(entry[1:3]), Current: entry[3], Worst: entry[4], RawHex: fmt.Sprintf("%012X", raw), RawValue: strconv.FormatUint(raw, 10), Source: "ATA SMART"}
		if v, ok := limits[id]; ok {
			threshold := v
			attribute.Threshold = &threshold
		}
		add := func(metricID, label, unit, explanation string, value float64) {
			attribute.Name, attribute.Unit, attribute.Interpretation = label, unit, explanation
			attribute.Value = &value
			m := model.Known(metricID, label, unit, "ATA SMART", value, metricID == "temperature")
			m.Description = explanation
			result.metrics = append(result.metrics, m)
		}
		if samsung || crucial || kingston {
			switch id {
			case 9:
				add("power-hours", "Наработка", "hours", "Время под питанием по счётчику устройства; режимы сна могут не учитываться.", float64(raw))
			case 12:
				add("power-cycles", "Включения", "", "Количество циклов питания по счётчику устройства.", float64(raw))
			}
		}
		if samsung {
			switch id {
			case 5:
				add("reallocated", "Переназначенные секторы", "", "Счётчик устройства; сам по себе не является итоговой оценкой здоровья.", float64(raw))
			case 177:
				add("wear-leveling", "Циклы износа NAND", "", "RAW счётчик Wear Leveling; нормализованный показатель приведён отдельно в таблице SMART.", float64(raw))
			case 179:
				add("used-reserved-blocks", "Использовано резервных блоков", "", "Счётчик резервных блоков производителя.", float64(raw))
			case 181, 182, 187, 199:
				ids := map[byte]string{181: "program-failures", 182: "erase-failures", 187: "uncorrectable-errors", 199: "sata-crc-errors"}
				labels := map[byte]string{181: "Ошибки программирования NAND", 182: "Ошибки стирания NAND", 187: "Неисправимые ошибки", 199: "Ошибки передачи SATA (CRC)"}
				add(ids[id], labels[id], "", "Счётчик ошибок устройства; CRC относится к передаче данных по интерфейсу.", float64(raw))
			case 190:
				v := float64(entry[5])
				if v > 0 && v <= 150 {
					add("temperature", "Температура", "°C", "Текущая температура из младшего RAW байта Samsung SMART BE.", v)
				}
			case 241:
				add("lifetime-written", "Записано за всё время", "bytes", "Samsung F1: RAW × 512 байт; это объём записей хоста, не оценка оставшегося ресурса.", float64(raw)*512)
			}
		}
		if crucial {
			switch id {
			case 5:
				add("reallocated-blocks", "Переназначенные блоки NAND", "", "Для этого семейства Micron атрибут 05 считает блоки NAND, не секторы HDD.", float64(raw))
			case 173:
				add("wear-leveling", "Средние циклы стирания NAND", "", "RAW среднее число циклов стирания; не процент здоровья.", float64(raw))
			case 174:
				add("unexpected-power-loss", "Внезапные отключения питания", "", "Счётчик неожиданных потерь питания.", float64(raw))
			case 187, 197, 198, 199:
				ids := map[byte]string{187: "uncorrectable-errors", 197: "pending-ecc", 198: "offline-uncorrectable", 199: "sata-crc-errors"}
				labels := map[byte]string{187: "Неисправимые ошибки", 197: "Ожидающие события ECC", 198: "Ошибки фонового сканирования", 199: "Ошибки передачи SATA (CRC)"}
				add(ids[id], labels[id], "", "RAW счётчик Micron TN-FD-22 для указанного семейства.", float64(raw))
			case 194:
				v := float64(binary.LittleEndian.Uint16(entry[5:7]))
				if v > 0 && v <= 150 {
					add("temperature", "Температура", "°C", "Micron C2: текущая температура RAW bytes 0–1; остальные байты содержат min/max.", v)
				}
			case 202:
				if attribute.Current <= 100 {
					add("remaining-life", "Расчётный ресурс NAND", "%", "Нормализованное Current атрибута CA; RAW отдельно содержит использованный ресурс. Не итоговая оценка здоровья диска.", float64(attribute.Current))
				}
			case 246:
				add("total-lbas-written", "Записано логических секторов", "LBA", "Micron F6: число логических секторов. Перевод в байты не выполняется без подтверждённой единицы.", float64(raw))
			}
		}
		if kingston && id == 231 && attribute.Current <= 100 {
			add("wear-indicator", "Индикатор ресурса SSD", "", "Нормализованный Kingston E7: 100 у нового SSD, 1 при достижении гарантийного предела; RAW не интерпретируется как процент.", float64(attribute.Current))
		}
		if wdHDD {
			switch id {
			case 5:
				add("reallocated", "Переназначенные секторы", "", "WD HDD: младшие 16 бит RAW; полные шесть байт сохранены в таблице.", float64(raw&0xffff))
			case 12:
				add("power-cycles", "Включения", "", "Количество циклов питания по счётчику устройства.", float64(raw))
			case 197, 198, 199:
				ids := map[byte]string{197: "pending-sectors", 198: "offline-uncorrectable", 199: "sata-crc-errors"}
				labels := map[byte]string{197: "Нестабильные секторы", 198: "Неисправимые секторы", 199: "Ошибки передачи SATA (CRC)"}
				add(ids[id], labels[id], "", "RAW счётчик WD HDD; не итоговая оценка здоровья диска.", float64(raw))
			case 241:
				add("total-lbas-written", "Записано логических секторов", "LBA", "WD F1: счётчик LBA. Единица байтов не предполагается.", float64(raw))
			}
		}
		result.attributes = append(result.attributes, attribute)
	}
	if len(result.attributes) == 0 {
		return diskHealthResult{}, ataInvalid("ATA_SMART_EMPTY")
	}
	return result, nil
}
