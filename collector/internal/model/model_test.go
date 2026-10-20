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
