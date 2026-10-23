package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"nexus/collector/internal/core"
	"nexus/collector/internal/platform"
	"os"
	"os/signal"
)

func main() {
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
	defer cancel()
	commands := make(chan core.Command, 8)
	// EOF is the parent-liveness signal. Main exits even if a Windows provider is hung.
	go readCommands(ctx, cancel, os.Stdin, commands)
	done := make(chan error, 1)
	go func() {
		a, err := platform.New()
		if err != nil {
			done <- err
			return
		}
		done <- core.Run(ctx, a, os.Stdout, commands)
	}()
	select {
	case <-ctx.Done():
		return
	case err := <-done:
		if err != nil {
			fmt.Fprintln(os.Stderr, "COLLECTOR_RUN_FAILED")
			os.Exit(1)
		}
	}
}

// Never block reading the parent pipe behind a full command queue: EOF must
// still terminate the process even while a provider is stuck initializing.
func readCommands(ctx context.Context, cancel context.CancelFunc, input io.Reader, commands chan<- core.Command) {
	s := bufio.NewScanner(input)
	s.Buffer(make([]byte, 1024), 4096)
	for s.Scan() {
		var c core.Command
		if json.Unmarshal(s.Bytes(), &c) == nil {
			if c.Command == "stop" {
				cancel()
				return
			}
			select {
			case commands <- c:
			case <-ctx.Done():
				return
			default:
				// Intervals are advisory; a saturated queue cannot delay stop/EOF.
			}
		}
	}
	cancel()
}
