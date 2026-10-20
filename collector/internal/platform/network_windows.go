//go:build windows

package platform

import (
	"context"
	"fmt"
	"golang.org/x/sys/windows"
	"net"
	"nexus/collector/internal/model"
	"sort"
	"strings"
	"time"
	"unsafe"
)

func (a *windowsAdapter) sampleNetwork(ctx context.Context, c *model.Component, now time.Time) {
	// IP/interface discovery traverses Windows tables: cache it on the slow lane.
	// Per-interval traffic uses only the inexpensive existing IP Helper counters.
	if a.lastNetwork.IsZero() || now.Sub(a.lastNetwork) >= a.profile.Slow {
		interfaces, err := net.Interfaces()
		a.lastNetwork = now
		a.interfaces, a.interfaceSpecs = nil, nil
		if err == nil {
			a.interfaces = interfaces
			a.interfaceSpecs = networkConfiguration()
		}
	}
	interfaces := a.interfaces
	c.Specs = append([]model.Spec{}, a.interfaceSpecs...)
	c.Metrics = []model.Metric{}
	if interfaces == nil {
		c.Name = "Сеть"
		c.Subtitle = "Обновление задерживается"
		missing(c, "rx", "Получение", "B/s", "Нет ответа от сетевых адаптеров", true)
		missing(c, "tx", "Отправка", "B/s", "Нет ответа от сетевых адаптеров", true)
		a.previous = map[string]windows.MibIfRow2{}
		a.previousAt = now
		return
	}
	rows := map[string]windows.MibIfRow2{}
	physical, virtual := []string{}, []string{}
	for _, n := range interfaces {
		if n.Flags&net.FlagLoopback != 0 || ctx.Err() != nil {
			continue
		}
		r := windows.MibIfRow2{InterfaceIndex: uint32(n.Index)}
		if windows.GetIfEntry2Ex(0, &r) != nil {
			continue
		}
		rows[n.Name] = r
		if r.OperStatus == 1 {
			if r.InterfaceAndOperStatusFlags&1 != 0 {
				physical = append(physical, n.Name)
			} else {
				virtual = append(virtual, n.Name)
			}
		}
	}
	selected := physical
	if len(selected) == 0 {
		selected = virtual
	}
	sort.Strings(selected)
	selectedSet := map[string]bool{}
	for _, n := range selected {
		selectedSet[n] = true
	}
	var rxTotal, txTotal, errorsTotal, discardsTotal float64
	complete := len(selected) > 0
	for _, n := range interfaces {
		r, ok := rows[n.Name]
		if !ok {
			continue
		}
		prefix := fmt.Sprintf("nic-%d", n.Index)
		online := r.OperStatus == 1
		kind := "Физический"
		if r.InterfaceAndOperStatusFlags&1 == 0 {
			kind = "Виртуальный"
		}
		status := "Отключён"
		if online {
			status = "Подключён"
		}
		c.Spec(n.Name, kind+" · "+status)
		add := func(suffix, label, unit string, v float64) {
			c.Set(model.Known(prefix+suffix, n.Name+" · "+label, unit, "Windows IP Helper", v, false))
		}
		if online && r.ReceiveLinkSpeed > 0 {
			add("-link", "Скорость соединения", "bit/s", float64(r.ReceiveLinkSpeed))
		}
		add("-received", "Получено", "bytes", float64(r.InOctets))
		add("-sent", "Отправлено", "bytes", float64(r.OutOctets))
		add("-errors", "Ошибки", "", float64(r.InErrors+r.OutErrors))
		add("-inErrors", "Ошибки приёма", "", float64(r.InErrors))
		add("-outErrors", "Ошибки передачи", "", float64(r.OutErrors))
		add("-discards", "Отброшенные пакеты", "", float64(r.InDiscards+r.OutDiscards))
		if selectedSet[n.Name] {
			errorsTotal += float64(r.InErrors) + float64(r.OutErrors)
			discardsTotal += float64(r.InDiscards) + float64(r.OutDiscards)
		}
		if prev, ok := a.previous[n.Name]; ok && online && prev.OperStatus == 1 && prev.InterfaceLuid == r.InterfaceLuid {
			rx := model.Rate(prev.InOctets, r.InOctets, now.Sub(a.previousAt))
			tx := model.Rate(prev.OutOctets, r.OutOctets, now.Sub(a.previousAt))
			if rx != nil && tx != nil {
				add("-rx", "Приём", "B/s", *rx)
				add("-tx", "Отправка", "B/s", *tx)
				if selectedSet[n.Name] {
					rxTotal += *rx
					txTotal += *tx
				}
			} else if selectedSet[n.Name] {
				complete = false
			}
		} else if selectedSet[n.Name] {
			complete = false
		}
	}
	c.Name = "Нет активного подключения"
	c.Set(model.Known("activeAdapters", "Активные адаптеры", "", "Windows IP Helper", float64(len(selected)), false))
	if len(selected) > 0 {
		c.Set(model.Known("errors", "Ошибки активных адаптеров", "", "Windows IP Helper", errorsTotal, false))
		c.Set(model.Known("discards", "Отброшенные пакеты активных адаптеров", "", "Windows IP Helper", discardsTotal, false))
	}
	c.Subtitle = "Сетевые адаптеры"
	if len(selected) > 0 {
		c.Name = strings.Join(selected, " + ")
		c.Subtitle = "Физическое подключение"
		if len(physical) == 0 {
			c.Subtitle = "Виртуальное подключение"
		}
		if len(selected) == 1 {
			r := rows[selected[0]]
			if r.ReceiveLinkSpeed > 0 {
				c.Set(model.Known("link", "Скорость соединения", "bit/s", "Windows IP Helper", float64(r.ReceiveLinkSpeed), true))
			}
		}
	}
	c.Note = "Главные показатели относятся к физическим подключениям. Виртуальные адаптеры и VPN показаны отдельно в подробностях. Скорость соединения — пропускная способность локального подключения, не тест скорости интернета."
	if complete {
		c.Set(model.Known("rx", "Получение", "B/s", "Windows IP Helper", rxTotal, true))
		c.Set(model.Known("tx", "Отправка", "B/s", "Windows IP Helper", txTotal, true))
	} else {
		missing(c, "rx", "Получение", "B/s", "Ожидание измерения подключения", true)
		missing(c, "tx", "Отправка", "B/s", "Ожидание измерения подключения", true)
	}
	a.previous = rows
	a.previousAt = now
}

// Only local configuration is read. This never sends a DNS, ICMP or internet
// request. The bounded retry handles adapters changing during enumeration.
func networkConfiguration() []model.Spec {
	size := uint32(15000)
	for attempt := 0; attempt < 3; attempt++ {
		if size > 1<<20 {
			return nil
		}
		buffer := make([]byte, size)
		first := (*windows.IpAdapterAddresses)(unsafe.Pointer(&buffer[0]))
		err := windows.GetAdaptersAddresses(windows.AF_UNSPEC, windows.GAA_FLAG_SKIP_ANYCAST|windows.GAA_FLAG_SKIP_MULTICAST|windows.GAA_FLAG_INCLUDE_GATEWAYS, 0, first, &size)
		if err == windows.ERROR_BUFFER_OVERFLOW {
			continue
		}
		if err != nil {
			return nil
		}
		out := []model.Spec{}
		for p := first; p != nil; p = p.Next {
			if p.IfType == 24 {
				continue
			} // Software loopback.
			name := windows.UTF16PtrToString(p.FriendlyName)
			for u := p.FirstUnicastAddress; u != nil; u = u.Next {
				if ip := u.Address.IP(); ip != nil {
					out = append(out, model.Spec{Label: name + " · IP", Value: ip.String()})
				}
			}
			for d := p.FirstDnsServerAddress; d != nil; d = d.Next {
				if ip := d.Address.IP(); ip != nil {
					out = append(out, model.Spec{Label: name + " · DNS", Value: ip.String()})
				}
			}
			for g := p.FirstGatewayAddress; g != nil; g = g.Next {
				if ip := g.Address.IP(); ip != nil {
					out = append(out, model.Spec{Label: name + " · Шлюз", Value: ip.String()})
				}
			}
		}
		return out
	}
	return nil
}
