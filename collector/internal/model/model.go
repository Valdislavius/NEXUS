// Package model is the versioned, OS-independent wire contract.
package model

import (
	"math"
	"strings"
	"time"
)

const Version = 1

// Diagnostic contains only a category and a bounded technical code, never device
// names, serials, paths, user identifiers, or a verbatim provider error.
type Diagnostic struct {
	Status string `json:"status"`
	Code   string `json:"code"`
}

type Metric struct {
	ID          string      `json:"id"`
	Label       string      `json:"label"`
	Value       *float64    `json:"value"`
	Unit        string      `json:"unit"`
	Source      string      `json:"source"`
	Reason      string      `json:"reason,omitempty"`
	At          string      `json:"at"`
	Status      string      `json:"status"`
	MaxAgeMS    int64       `json:"maxAgeMs,omitempty"`
	Primary     bool        `json:"primary"`
	Description string      `json:"description,omitempty"`
	Diagnostic  *Diagnostic `json:"diagnostic,omitempty"`
}
type SMARTAttribute struct {
	ID             uint8    `json:"id"`
	Name           string   `json:"name"`
	Flags          uint16   `json:"flags"`
	Current        uint8    `json:"current"`
	Worst          uint8    `json:"worst"`
	Threshold      *uint8   `json:"threshold,omitempty"`
	RawHex         string   `json:"rawHex"`
	RawValue       string   `json:"rawValue"`
	Interpretation string   `json:"interpretation,omitempty"`
	Unit           string   `json:"unit,omitempty"`
	Value          *float64 `json:"value,omitempty"`
	Source         string   `json:"source"`
}
type Spec struct {
	Label string `json:"label"`
	Value string `json:"value"`
}
type Component struct {
	ID       string           `json:"id"`
	Kind     string           `json:"kind"`
	Name     string           `json:"name"`
	Subtitle string           `json:"subtitle"`
	Specs    []Spec           `json:"specs"`
	Metrics  []Metric         `json:"metrics"`
	Note     string           `json:"note,omitempty"`
	SMART    []SMARTAttribute `json:"smart,omitempty"`
}
type Snapshot struct {
	Version    int         `json:"version"`
	Type       string      `json:"type"`
	At         string      `json:"at"`
	Host       string      `json:"host"`
	Platform   string      `json:"platform"`
	Ready      bool        `json:"ready"`
	Components []Component `json:"components"`
	Warnings   []string    `json:"warnings"`
}

func Now() string { return time.Now().UTC().Format(time.RFC3339Nano) }
func Known(id, label, unit, source string, value float64, primary bool) Metric {
	if math.IsNaN(value) || math.IsInf(value, 0) || value < 0 {
		return MissingDiagnostic(id, label, unit, source, "Источник вернул некорректное значение", "invalid_response", "VALUE_INVALID", primary)
	}
	return Metric{ID: id, Label: label, Unit: unit, Source: source, Value: &value, At: Now(), Status: "available", Primary: primary}
}
func Missing(id, label, unit, source, reason string, primary bool) Metric {
	return Metric{ID: id, Label: label, Unit: unit, Source: source, Reason: reason, At: Now(), Status: "unavailable", Primary: primary}
}

// Expire retains the acquisition timestamp, source and unit, but never forwards
// an old numeric reading as current telemetry after sleep or a stalled provider.
func (m *Metric) Expire(now time.Time) {
	if m.Value == nil || m.MaxAgeMS <= 0 {
		return
	}
	at, err := time.Parse(time.RFC3339Nano, m.At)
	if err == nil && now.Sub(at) >= 0 && now.Sub(at) <= time.Duration(m.MaxAgeMS)*time.Millisecond {
		return
	}
	m.Value = nil
	m.Status = "stale"
	m.Reason = "Данные устарели; ожидание нового измерения"
	m.Diagnostic = &Diagnostic{Status: "stale", Code: "METRIC_EXPIRED"}
}
func MissingDiagnostic(id, label, unit, source, reason, status, code string, primary bool) Metric {
	m := Missing(id, label, unit, source, reason, primary)
	m.Diagnostic = &Diagnostic{Status: status, Code: code}
	return m
}
func (c *Component) Set(m Metric) {
	for i := range c.Metrics {
		if c.Metrics[i].ID == m.ID {
			c.Metrics[i] = m
			return
		}
	}
	c.Metrics = append(c.Metrics, m)
}
func (c *Component) Spec(label, value string) {
	if strings.TrimSpace(value) == "" || strings.Contains(value, "Недоступно") || strings.EqualFold(value, "x.x") || strings.EqualFold(value, "To Be Filled By O.E.M.") || strings.EqualFold(value, "Default string") || strings.EqualFold(value, "Unknown") {
		return
	}
	c.Specs = append(c.Specs, Spec{label, value})
}
func New(id, kind, name, subtitle string) Component {
	return Component{ID: id, Kind: kind, Name: name, Subtitle: subtitle, Specs: []Spec{}, Metrics: []Metric{}}
}

// Rate rejects first samples, counter resets, and non-monotonic timestamps.
func Rate(before, after uint64, elapsed time.Duration) *float64 {
	if after < before || elapsed <= 0 {
		return nil
	}
	v := float64(after-before) / elapsed.Seconds()
	return &v
}
