package core

import (
	"bytes"
	"context"
	"encoding/json"
	"nexus/collector/internal/model"
	"nexus/collector/internal/platform"
	"testing"
	"time"
)

type fakeAdapter struct {
	closed               bool
	inventories, samples int
	profiles             []platform.CollectionProfile
	intervals            []time.Duration
}

func (a *fakeAdapter) SetProfile(p platform.CollectionProfile) { a.profiles = append(a.profiles, p) }
func (a *fakeAdapter) SetInterval(d time.Duration)             { a.intervals = append(a.intervals, d) }

func (a *fakeAdapter) Inventory(context.Context) ([]model.Component, []string) {
	a.inventories++
	return []model.Component{model.New("cpu", "cpu", "Test", "")}, []string{}
}

func TestProfileCommandsAreValidatedAndLegacyIntervalPreserved(t *testing.T) {
	a := &fakeAdapter{}
	commands := make(chan Command, 6)
	for _, command := range []Command{
		{Command: "profile", Profile: "economy"}, {Command: "profile", Profile: "invalid"},
		{Command: "interval", IntervalMS: 1}, {Command: "interval", IntervalMS: 1000},
		{Command: "profile", Profile: "detailed"}, {Command: "stop"},
	} {
		commands <- command
	}
	if err := Run(context.Background(), a, &bytes.Buffer{}, commands); err != nil {
		t.Fatal(err)
	}
	if len(a.profiles) != 3 || a.profiles[0].Name != "balanced" || a.profiles[1].Slow != 60*time.Second || a.profiles[2].Name != "detailed" {
		t.Fatalf("unbounded/invalid profile applied: %+v", a.profiles)
	}
	if len(a.intervals) != 1 || a.intervals[0] != time.Second {
		t.Fatalf("invalid legacy interval handling: %+v", a.intervals)
	}
}
func (a *fakeAdapter) Sample(_ context.Context, c []model.Component) ([]model.Component, []string) {
	a.samples++
	return c, []string{}
}
func (a *fakeAdapter) Close() { a.closed = true }
func TestRunStopAndProtocol(t *testing.T) {
	a := &fakeAdapter{}
	var out bytes.Buffer
	commands := make(chan Command, 1)
	commands <- Command{Command: "stop"}
	if err := Run(context.Background(), a, &out, commands); err != nil {
		t.Fatal(err)
	}
	if !a.closed || a.inventories != 1 || a.samples != 1 {
		t.Fatalf("lifecycle %+v", a)
	}
	d := json.NewDecoder(&out)
	var first, second model.Snapshot
	if d.Decode(&first) != nil || d.Decode(&second) != nil {
		t.Fatal("invalid JSON framing")
	}
	if first.Ready || !second.Ready || second.Version != 1 {
		t.Fatal("invalid lifecycle snapshots")
	}
}
