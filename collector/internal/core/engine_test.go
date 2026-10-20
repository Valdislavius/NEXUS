package core

import (
	"bytes"
	"context"
	"encoding/json"
	"nexus/collector/internal/model"
	"testing"
)

type fakeAdapter struct {
	closed               bool
	inventories, samples int
}

func (a *fakeAdapter) Inventory(context.Context) ([]model.Component, []string) {
	a.inventories++
	return []model.Component{model.New("cpu", "cpu", "Test", "")}, []string{}
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
