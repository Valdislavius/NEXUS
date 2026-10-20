//go:build windows

package platform

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/shirou/gopsutil/v4/cpu"

	"github.com/shirou/gopsutil/v4/host"
	"github.com/shirou/gopsutil/v4/mem"
	"github.com/yusufpapurcu/wmi"
	"golang.org/x/sys/windows"
	"nexus/collector/internal/model"
)

type windowsAdapter struct {
	thermometer    cpuThermometer
	singleCPU      bool
	previousDisks  map[string]physicalCounters
	previousDiskAt time.Time
	nv             *nvml
	volumes        map[string][]string
	previous       map[string]windows.MibIfRow2
	previousAt     time.Time
	lastVolumes    time.Time
	lastHealth     time.Time
	warnings       []string
	primed         bool
}

func New() (Adapter, error) {
	return &windowsAdapter{nv: openNVML(), volumes: map[string][]string{}, previous: map[string]windows.MibIfRow2{}}, nil
}
func (a *windowsAdapter) Close() { a.nv.close(); a.thermometer.close() }
func number(v uint64) string {
	if v == 0 {
		return ""
	}
	return strconv.FormatUint(v, 10)
}
func capacity(v uint64) string {
	if v == 0 {
		return ""
	}
	return strings.ReplaceAll(fmt.Sprintf("%.1f ГиБ", float64(v)/(1<<30)), ".", ",")
}
func missing(c *model.Component, id, label, unit, why string, primary bool) {
	c.Set(model.Missing(id, label, unit, "Windows", why, primary))
}
func query[T any](q string) ([]T, error) { var rows []T; err := wmi.Query(q, &rows); return rows, err }
func (a *windowsAdapter) warn(label string, err error) {
	if err != nil {
		a.warnings = append(a.warnings, label+": источник Windows недоступен")
	}
}

type processor struct {
	Name, Manufacturer, SocketDesignation                                             string
	NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed, L2CacheSize, L3CacheSize uint32
}
type memoryModule struct {
	Manufacturer, PartNumber, DeviceLocator string
	Capacity                                uint64
	Speed, ConfiguredClockSpeed             uint32
}
type video struct{ Name, DriverVersion, PNPDeviceID string }
type physicalDisk struct {
	Index                                     uint32
	Model, InterfaceType, MediaType, DeviceID string
	Size                                      uint64
}
type partition struct {
	DeviceID string
	Index    uint32
	Size     uint64
}
type logicalDisk struct {
	DeviceID, FileSystem, VolumeName string
	Size, FreeSpace                  uint64
}
type baseboard struct{ Manufacturer, Product, Version string }
type bios struct {
	SMBIOSBIOSVersion, Manufacturer string
	ReleaseDate                     time.Time
}
type operatingSystem struct{ Caption, Version, BuildNumber, OSArchitecture string }

func (a *windowsAdapter) Inventory(ctx context.Context) ([]model.Component, []string) {
	// Rediscovery must release a previous CPU driver handle before selecting the
	// current vendor and package again.
	a.thermometer.close()
	result := []model.Component{}
	cpus, err := query[processor]("SELECT Name,Manufacturer,SocketDesignation,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,L2CacheSize,L3CacheSize FROM Win32_Processor")
	a.warn("CPU", err)
	a.singleCPU = err == nil && len(cpus) == 1
	c := model.New("cpu", "cpu", "Процессор", "Вычислительные ресурсы")
	if len(cpus) > 0 {
		names := []string{}
		cores, threads := uint32(0), uint32(0)
		for _, r := range cpus {
			names = append(names, strings.TrimSpace(r.Name))
			cores += r.NumberOfCores
			threads += r.NumberOfLogicalProcessors
		}
		c.Name = strings.Join(names, " + ")
		r := cpus[0]
		c.Subtitle = fmt.Sprintf("%d ядер · %d потоков", cores, threads)
		c.Spec("Ядра / логические процессоры", fmt.Sprintf("%d / %d", cores, threads))
		manufacturer := strings.TrimSpace(r.Manufacturer)
		if manufacturer == "GenuineIntel" {
			manufacturer = "Intel"
		}
		if manufacturer == "AuthenticAMD" {
			manufacturer = "AMD"
		}
		c.Spec("Производитель", manufacturer)
		if !strings.EqualFold(strings.TrimSpace(r.SocketDesignation), "U3E1") {
			c.Spec("Сокет", r.SocketDesignation)
		}
		c.Spec("Макс. частота по WMI", number(uint64(r.MaxClockSpeed))+" МГц")
		c.Spec("Кэш L2 (WMI)", number(uint64(r.L2CacheSize))+" КиБ")
		c.Spec("Кэш L3 (WMI)", number(uint64(r.L3CacheSize))+" КиБ")
	}
	missing(&c, "load", "Загрузка", "%", "Ожидание второго измерения", true)
	missing(&c, "clock", "Частота Windows", "МГц", "Не получена", true)
	missing(&c, "temperature", "Температура", "°C", "Ожидание CPU Package", true)
	c.Spec("Датчик температуры", a.thermometer.description())
	c.Note = "Частота сообщается системой управления питанием Windows и может отличаться от эффективной частоты под нагрузкой. Для температуры Intel CPU Package нужен установленный PawnIO. Температура AMD пока не поддерживается в безопасном режиме чтения. Драйвер автоматически не устанавливается."
	result = append(result, c)
	if ctx.Err() != nil {
		return result, a.warnings
	}
	videos, err := query[video]("SELECT Name,DriverVersion,PNPDeviceID FROM Win32_VideoController")
	a.warn("GPU", err)
	nvCards := a.nv.inventory()
	used := map[int]bool{}
	for i, r := range videos {
		g := model.New(fmt.Sprintf("gpu-%d", i), "gpu", r.Name, "Графический адаптер")
		// Match each NVML device once. Identical-name cards retain NVML's own indices.
		for j, n := range nvCards {
			if !used[j] && strings.EqualFold(strings.TrimSpace(n.Name), strings.TrimSpace(r.Name)) {
				g = n
				used[j] = true
				break
			}
		}
		g.Spec("Драйвер Windows", r.DriverVersion)
		if !strings.HasPrefix(g.ID, "nv-") {
			gpuUnavailable(&g)
		}
		result = append(result, g)
	}
	for j, g := range nvCards {
		if !used[j] {
			result = append(result, g)
		}
	}
	if len(videos) == 0 && len(nvCards) == 0 {
		g := model.New("gpu", "gpu", "Видеокарта", "Сведения недоступны")
		gpuUnavailable(&g)
		result = append(result, g)
	}
	ram := model.New("ram", "ram", "Оперативная память", "Физическая память")
	modules, err := query[memoryModule]("SELECT Manufacturer,PartNumber,DeviceLocator,Capacity,Speed,ConfiguredClockSpeed FROM Win32_PhysicalMemory")
	a.warn("Модули RAM", err)
	var total uint64
	speeds := map[uint32]bool{}
	for _, r := range modules {
		total += r.Capacity
		rate := r.ConfiguredClockSpeed
		if rate == 0 {
			rate = r.Speed
		}
		speed := "Недоступно"
		if rate > 0 {
			speeds[rate] = true
			speed = fmt.Sprintf("%d MT/s (SMBIOS)", rate)
		}
		ram.Spec(r.DeviceLocator, fmt.Sprintf("%s · %s · %s · %s", strings.TrimSpace(r.Manufacturer), strings.TrimSpace(r.PartNumber), capacity(r.Capacity), speed))
	}
	if total > 0 {
		ram.Name = capacity(total) + " RAM"
		ram.Subtitle = fmt.Sprintf("Модулей: %d", len(modules))
		if len(speeds) == 1 {
			for rate := range speeds {
				ram.Subtitle += fmt.Sprintf(" · %d MT/s", rate)
			}
		}
	}
	missing(&ram, "used", "Используется", "bytes", "Ожидание измерения", true)
	missing(&ram, "load", "Загрузка", "%", "Ожидание измерения", true)
	missing(&ram, "available", "Доступно", "bytes", "Ожидание измерения", true)
	result = append(result, ram)
	drives, err := query[physicalDisk]("SELECT Index,Model,InterfaceType,MediaType,DeviceID,Size FROM Win32_DiskDrive")
	a.warn("Накопители", err)
	for _, r := range drives {
		id := fmt.Sprintf("disk-%d", r.Index)
		d := model.New(id, "storage", strings.TrimSpace(r.Model), "Физический диск "+strconv.Itoa(int(r.Index))+" · "+capacity(r.Size))
		d.Spec("Ёмкость", capacity(r.Size))
		bus := deviceBus(r.Index)
		if bus == "" {
			bus = r.InterfaceType
		}
		d.Spec("Интерфейс", bus)
		d.Subtitle = bus + " · " + capacity(r.Size)
		d.Set(model.Known("capacity", "Ёмкость", "bytes", "Win32_DiskDrive", float64(r.Size), false))
		parts, e := query[partition](fmt.Sprintf("SELECT DeviceID,Index,Size FROM Win32_DiskPartition WHERE DiskIndex=%d", r.Index))
		a.warn("Разделы "+id, e)
		seen := map[string]bool{}
		for _, p := range parts {
			d.Spec(fmt.Sprintf("Раздел %d", p.Index+1), capacity(p.Size)+" · "+p.DeviceID)
			vols, e := query[logicalDisk]("ASSOCIATORS OF {Win32_DiskPartition.DeviceID='" + p.DeviceID + "'} WHERE AssocClass=Win32_LogicalDiskToPartition")
			a.warn("Тома "+id, e)
			for _, v := range vols {
				if !seen[v.DeviceID] {
					seen[v.DeviceID] = true
					a.volumes[id] = append(a.volumes[id], v.DeviceID)
					volumeLabel := strings.TrimSpace(v.VolumeName)
					if volumeLabel != "" {
						volumeLabel += " · "
					}
					d.Spec("Том "+v.DeviceID, volumeLabel+v.FileSystem+" · "+capacity(v.Size))
					missing(&d, "volume-"+v.DeviceID, "Занято "+v.DeviceID, "bytes", "Ожидание измерения", false)
				}
			}
		}
		missing(&d, "used", "Занято на томах", "bytes", "Нет доступных томов с буквами диска", true)
		// Health sensors are discovered from the actual device.
		missing(&d, "read", "Чтение", "B/s", "Ожидание измерения", false)
		missing(&d, "write", "Запись", "B/s", "Ожидание измерения", false)
		d.Note = "Ёмкость относится к физическому диску; занятое пространство — к связанным томам с буквами. Составные тома и Storage Spaces могут иметь неполное сопоставление."
		result = append(result, d)
	}
	if len(drives) == 0 {
		d := model.New("storage", "storage", "Накопители", "Сведения недоступны")
		missing(&d, "used", "Занято", "bytes", "Не удалось определить физические диски", true)
		result = append(result, d)
	}
	board := model.New("board", "motherboard", "Материнская плата", "Платформа и прошивка")
	boards, err := query[baseboard]("SELECT Manufacturer,Product,Version FROM Win32_BaseBoard")
	a.warn("Материнская плата", err)
	for _, r := range boards {
		board.Name = strings.TrimSpace(r.Manufacturer + " " + r.Product)
		board.Spec("Производитель", r.Manufacturer)
		board.Spec("Модель", r.Product)
		board.Spec("Ревизия", r.Version)
	}
	bioses, err := query[bios]("SELECT SMBIOSBIOSVersion,Manufacturer,ReleaseDate FROM Win32_BIOS")
	a.warn("BIOS", err)
	for _, r := range bioses {
		board.Subtitle = "BIOS " + r.SMBIOSBIOSVersion
		board.Spec("Версия BIOS", r.SMBIOSBIOSVersion)
		if !r.ReleaseDate.IsZero() {
			board.Spec("Дата BIOS", r.ReleaseDate.Format("02.01.2006"))
		}
	}
	result = append(result, board)
	system := model.New("system", "system", "Windows", "Операционная система")
	systems, err := query[operatingSystem]("SELECT Caption,Version,BuildNumber,OSArchitecture FROM Win32_OperatingSystem")
	a.warn("Система", err)
	for _, r := range systems {
		system.Name = r.Caption
		system.Subtitle = r.OSArchitecture + " · сборка " + r.BuildNumber
		system.Spec("Версия", r.Version)
		system.Spec("Сборка", r.BuildNumber)
		system.Spec("Архитектура", r.OSArchitecture)
	}
	missing(&system, "uptime", "Время работы", "seconds", "Ожидание измерения", true)
	system.Spec("Режим NEXUS", "Локально · без передачи данных")
	result = append(result, system)
	network := model.New("network", "network", "Сеть", "Адаптеры обновляются при каждом опросе")
	result = append(result, network)
	return result, append([]string{}, a.warnings...)
}

func (a *windowsAdapter) Sample(ctx context.Context, components []model.Component) ([]model.Component, []string) {
	warnings := append([]string{}, a.warnings...)
	loads, loadErr := cpu.PercentWithContext(ctx, 0, true)
	memory, memErr := mem.VirtualMemoryWithContext(ctx)
	clocks := powerClocks()
	now := time.Now()
	refreshHealth := now.Sub(a.lastHealth) >= 30*time.Second
	refreshVolumes := now.Sub(a.lastVolumes) >= 30*time.Second
	var volumeRows []logicalDisk
	if refreshVolumes {
		var e error
		volumeRows, e = query[logicalDisk]("SELECT DeviceID,FileSystem,VolumeName,Size,FreeSpace FROM Win32_LogicalDisk WHERE DriveType=3")
		if e != nil {
			warnings = append(warnings, "Свободное место: источник недоступен")
		}
		a.lastVolumes = now
	}
	for i := range components {
		c := &components[i]
		switch c.Kind {
		case "cpu":
			c.Set(sampleCPUTemperature(&a.thermometer, a.singleCPU))
			if loadErr == nil && a.primed && len(loads) > 0 {
				total := 0.0
				for j, v := range loads {
					total += v
					c.Set(model.Known(fmt.Sprintf("thread-%d", j), fmt.Sprintf("Поток %02d", j), "%", "Windows Processor Performance", v, false))
				}
				c.Set(model.Known("load", "Загрузка", "%", "Windows Processor Performance", total/float64(len(loads)), true))
			} else {
				missing(c, "load", "Загрузка", "%", "Ожидание измерения или ошибка счётчика", true)
				for j := range c.Metrics {
					if strings.HasPrefix(c.Metrics[j].ID, "thread-") {
						c.Metrics[j] = model.Missing(c.Metrics[j].ID, c.Metrics[j].Label, "%", "Windows", "Счётчик недоступен", false)
					}
				}
			}
			if len(clocks) > 0 {
				sum := 0.0
				for _, v := range clocks {
					sum += v
				}
				c.Set(model.Known("clock", "Частота Windows", "МГц", "CallNtPowerInformation", sum/float64(len(clocks)), true))
			} else {
				missing(c, "clock", "Частота Windows", "МГц", "API управления питанием недоступен", true)
			}
		case "ram":
			if memErr == nil {
				c.Set(model.Known("used", "Используется", "bytes", "GlobalMemoryStatusEx", float64(memory.Total-memory.Available), true))
				c.Set(model.Known("load", "Загрузка", "%", "GlobalMemoryStatusEx", 100*float64(memory.Total-memory.Available)/float64(memory.Total), true))
				c.Set(model.Known("available", "Доступно", "bytes", "GlobalMemoryStatusEx", float64(memory.Available), true))
				c.Set(model.Known("total", "Доступно Windows всего", "bytes", "GlobalMemoryStatusEx", float64(memory.Total), false))
			} else {
				for _, m := range c.Metrics {
					missing(c, m.ID, m.Label, m.Unit, "Не удалось прочитать физическую память", m.Primary)
				}
			}
		case "gpu":
			a.nv.sample(c)
		case "storage":
			if refreshHealth {
				index, e := strconv.ParseUint(strings.TrimPrefix(c.ID, "disk-"), 10, 32)
				if e == nil {
					applyDiskHealth(c, uint32(index))
				}
			}
			if refreshVolumes {
				// Discard the previous sample before accepting fresh volume values.
				for j, m := range c.Metrics {
					if m.Source == "Win32_LogicalDisk" {
						c.Metrics[j] = model.Missing(m.ID, m.Label, m.Unit, m.Source, "Том недоступен", m.Primary)
					}
				}
				var used, total uint64
				count := 0
				for _, letter := range a.volumes[c.ID] {
					found := false
					for _, v := range volumeRows {
						if v.DeviceID == letter && v.Size >= v.FreeSpace {
							used += v.Size - v.FreeSpace
							total += v.Size
							c.Set(model.Known("volume-total-"+letter, "Ёмкость "+letter, "bytes", "Win32_LogicalDisk", float64(v.Size), false))
							c.Set(model.Known("volume-free-"+letter, "Свободно "+letter, "bytes", "Win32_LogicalDisk", float64(v.FreeSpace), false))
							count++
							found = true
							c.Set(model.Known("volume-"+letter, "Занято "+letter, "bytes", "Win32_LogicalDisk", float64(v.Size-v.FreeSpace), false))
							break
						}
					}
					if !found {
						missing(c, "volume-"+letter, "Занято "+letter, "bytes", "Том недоступен", false)
					}
				}
				if count > 0 && count == len(a.volumes[c.ID]) {
					c.Set(model.Known("used", "Занято на томах", "bytes", "Win32_LogicalDisk", float64(used), true))
					c.Set(model.Known("free", "Свободно", "bytes", "Win32_LogicalDisk", float64(total-used), false))
					c.Set(model.Known("volumeCapacity", "Ёмкость томов", "bytes", "Win32_LogicalDisk", float64(total), false))
					if total > 0 {
						c.Set(model.Known("load", "Занято", "%", "Win32_LogicalDisk", float64(used)/float64(total)*100, false))
					}
				} else {
					missing(c, "used", "Занято на томах", "bytes", "Нет полного набора доступных томов", true)
				}
			}
		case "system":
			if up, e := host.UptimeWithContext(ctx); e == nil {
				c.Set(model.Known("uptime", "Время работы", "seconds", "GetTickCount64", float64(up), true))
			} else {
				missing(c, "uptime", "Время работы", "seconds", "Счётчик недоступен", true)
			}
		case "network":
			a.sampleNetwork(ctx, c, now)
		}
	}
	if refreshHealth {
		a.lastHealth = now
	}
	a.primed = loadErr == nil
	// Native disk I/O counters are sampled without repeated static WMI queries.
	a.sampleDiskIO(ctx, components, now)
	return components, warnings
}

// Counters are read from physical device handles, never inferred from volumes.
func (a *windowsAdapter) sampleDiskIO(ctx context.Context, components []model.Component, now time.Time) {
	next := map[string]physicalCounters{}
	for i := range components {
		c := &components[i]
		if c.Kind != "storage" {
			continue
		}
		missing(c, "read", "Чтение", "B/s", "Ожидание второго измерения или счётчик недоступен", false)
		missing(c, "write", "Запись", "B/s", "Ожидание второго измерения или счётчик недоступен", false)
		if ctx.Err() != nil {
			continue
		}
		index, err := strconv.ParseUint(strings.TrimPrefix(c.ID, "disk-"), 10, 32)
		if err != nil {
			continue
		}
		current, ok := readPhysicalCounters(uint32(index))
		if !ok {
			continue
		}
		next[c.ID] = current
		old, was := a.previousDisks[c.ID]
		if !was {
			continue
		}
		rx := model.Rate(old.read, current.read, now.Sub(a.previousDiskAt))
		tx := model.Rate(old.write, current.write, now.Sub(a.previousDiskAt))
		if rx != nil && tx != nil {
			c.Set(model.Known("read", "Чтение", "B/s", "IOCTL_DISK_PERFORMANCE", *rx, false))
			c.Set(model.Known("write", "Запись", "B/s", "IOCTL_DISK_PERFORMANCE", *tx, false))
		}
	}
	a.previousDisks = next
	a.previousDiskAt = now
}
