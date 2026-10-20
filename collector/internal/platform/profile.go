package platform

import "time"

// CollectionProfile controls acquisition, not rendering. Slow, expensive sensor
// reads are cached with their original timestamp; interval updates never force WMI.
type CollectionProfile struct {
	Name string
	Fast time.Duration
	Slow time.Duration
}

func Profile(name string) (CollectionProfile, bool) {
	switch name {
	case "economy":
		return CollectionProfile{name, 5 * time.Second, 60 * time.Second}, true
	case "balanced":
		return CollectionProfile{name, 2 * time.Second, 30 * time.Second}, true
	case "detailed":
		return CollectionProfile{name, 2 * time.Second, 10 * time.Second}, true
	default:
		return CollectionProfile{}, false
	}
}

// Optional interfaces preserve the small, testable Adapter contract.
type ProfileAdapter interface{ SetProfile(CollectionProfile) }
type IntervalAdapter interface{ SetInterval(time.Duration) }
