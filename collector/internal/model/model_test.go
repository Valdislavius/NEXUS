package model

import (
	"encoding/json"
	"math"
	"testing"
	"time"
)

func TestMissingValues(t *testing.T) {
	for _, v := range []float64{math.NaN(), math.Inf(1), -1} {
		m := Known("x", "x", "%", "test", v, true)
		if m.Value != nil {
			t.Fatal("invalid value accepted")
		}
		if _, err := json.Marshal(m); err != nil {
			t.Fatal(err)
		}
	}
	if Known("x", "x", "%", "test", 0, true).Value == nil {
		t.Fatal("zero is valid")
	}
}
func TestRate(t *testing.T) {
	if Rate(10, 9, time.Second) != nil || Rate(1, 2, 0) != nil {
		t.Fatal("invalid counter interval")
	}
	if v := Rate(10, 30, 2*time.Second); v == nil || *v != 10 {
		t.Fatal("incorrect rate")
	}
}

func TestAvailabilityAndExpiration(t *testing.T) {
	now := time.Date(2026, 10, 14, 12, 0, 0, 0, time.UTC)
	m := Known("temperature", "Temperature", "°C", "test", 40, true)
	m.At, m.MaxAgeMS = now.Add(-5*time.Second).Format(time.RFC3339Nano), 6000
	m.Expire(now)
	if m.Status != "available" || m.Value == nil {
		t.Fatal("fresh cached sample discarded")
	}
	originalAt := m.At
	m.Expire(now.Add(2 * time.Second))
	if m.Status != "stale" || m.Value != nil || m.At != originalAt || m.Diagnostic == nil || m.Diagnostic.Code != "METRIC_EXPIRED" || m.Diagnostic.Status != "transient_error" {
		t.Fatalf("invalid stale sample: %+v", m)
	}
	if m := Missing("x", "X", "%", "test", "not supported", true); m.Status != "unavailable" || m.Value != nil {
		t.Fatal("missing sensor status")
	}
	for _, at := range []string{"invalid", now.Add(time.Minute).Format(time.RFC3339Nano)} {
		m := Known("x", "X", "%", "test", 0, true)
		m.At, m.MaxAgeMS = at, 5000
		m.Expire(now)
		if m.Value != nil || m.Status != "stale" {
			t.Fatal("invalid/future timestamp accepted")
		}
	}
}
