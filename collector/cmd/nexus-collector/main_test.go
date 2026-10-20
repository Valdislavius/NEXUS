package main

import (
	"context"
	"nexus/collector/internal/core"
	"strings"
	"testing"
)

func TestEOFAndStopCannotBlockOnFullCommandQueue(t *testing.T) {
	for _, ending := range []string{"", "{\"command\":\"stop\"}\n"} {
		ctx, cancel := context.WithCancel(context.Background())
		commands := make(chan core.Command, 1)
		commands <- core.Command{Command: "interval", IntervalMS: 2000}
		readCommands(ctx, cancel, strings.NewReader(strings.Repeat("{\"command\":\"interval\",\"intervalMs\":5000}\n", 20)+ending), commands)
		if ctx.Err() == nil {
			t.Fatal("parent EOF/stop did not cancel while commands were full")
		}
		cancel()
	}
}
