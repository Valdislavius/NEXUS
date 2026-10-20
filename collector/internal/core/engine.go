package core

import (
	"context"
	"encoding/json"
	"io"
	"nexus/collector/internal/model"
	"nexus/collector/internal/platform"
	"os"
	"runtime"
	"time"
)

type Command struct {
	Command    string `json:"command"`
	IntervalMS int    `json:"intervalMs"`
}

func Run(ctx context.Context, a platform.Adapter, out io.Writer, commands <-chan Command) error {
	defer a.Close()
	host, _ := os.Hostname()
	snap := model.Snapshot{Version: model.Version, Type: "snapshot", Host: host, Platform: runtime.GOOS, Components: []model.Component{}, Warnings: []string{}}
	enc := json.NewEncoder(out)
	emit := func() error { snap.At = model.Now(); return enc.Encode(snap) }
	if err := emit(); err != nil {
		return err
	}
	snap.Components, snap.Warnings = a.Inventory(ctx)
	snap.Ready = true
	if ctx.Err() != nil {
		return nil
	}
	interval := 2 * time.Second
	for {
		snap.Components, snap.Warnings = a.Sample(ctx, snap.Components)
		if err := emit(); err != nil {
			return err
		}
		timer := time.NewTimer(interval)
	wait:
		for {
			select {
			case <-ctx.Done():
				timer.Stop()
				return nil
			case cmd, ok := <-commands:
				if !ok {
					commands = nil
					continue
				}
				if cmd.Command == "interval" && (cmd.IntervalMS == 2000 || cmd.IntervalMS == 5000 || cmd.IntervalMS == 10000) {
					interval = time.Duration(cmd.IntervalMS) * time.Millisecond
					timer.Reset(interval)
				}
				if cmd.Command == "stop" {
					timer.Stop()
					return nil
				}
			case <-timer.C:
				break wait
			}
		}
	}
}
