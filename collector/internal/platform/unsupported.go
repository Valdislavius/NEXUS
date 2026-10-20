//go:build !windows

package platform

import "errors"

func New() (Adapter, error) {
	return nil, errors.New("Alpha 0.1 supports Windows only; this OS adapter is not implemented")
}
