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
)

func (a *windowsAdapter) sampleNetwork(ctx context.Context, c *model.Component, now time.Time) {
	interfaces, err := net.Interfaces()
	c.Specs = []model.Spec{}
	c.Metrics = []model.Metric{}
	if err != nil {
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
	var rxTotal, txTotal float64
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
		addresses, _ := n.Addrs()
		ips := []string{}
		for _, ip := range addresses {
			ips = append(ips, ip.String())
		}
		c.Spec(n.Name, kind+" · "+status+" · "+strings.Join(ips, ", "))
		add := func(suffix, label, unit string, v float64) {
			c.Set(model.Known(prefix+suffix, n.Name+" · "+label, unit, "Windows IP Helper", v, false))
		}
		if online && r.ReceiveLinkSpeed > 0 {
			add("-link", "Скорость соединения", "bit/s", float64(r.ReceiveLinkSpeed))
		}
		add("-received", "Получено", "bytes", float64(r.InOctets))
		add("-sent", "Отправлено", "bytes", float64(r.OutOctets))
		add("-errors", "Ошибки", "", float64(r.InErrors+r.OutErrors))
		if prev, ok := a.previous[n.Name]; ok && online {
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
