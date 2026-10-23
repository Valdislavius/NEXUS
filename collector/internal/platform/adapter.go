// Package platform defines the only interface an OS adapter must implement.
package platform

import (
	"context"
	"nexus/collector/internal/model"
)

type Adapter interface {
	Inventory(context.Context) ([]model.Component, []string)
	Sample(context.Context, []model.Component) ([]model.Component, []string)
	Close()
}
