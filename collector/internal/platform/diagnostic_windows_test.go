//go:build windows

package platform

import (
	"fmt"
	"testing"
)

func TestAssociations(t *testing.T) {
	ds, e := query[physicalDisk]("SELECT Index,Model,InterfaceType,MediaType,DeviceID,Size FROM Win32_DiskDrive")
	if e != nil {
		t.Fatal(e)
	}
	for _, d := range ds {
		rows, e := query[partition](fmt.Sprintf("SELECT DeviceID,Size FROM Win32_DiskPartition WHERE DiskIndex=%d", d.Index))
		t.Logf("disk %d parts %+v error %v", d.Index, rows, e)
		for _, p := range rows {
			vs, e := query[logicalDisk]("ASSOCIATORS OF {Win32_DiskPartition.DeviceID='" + p.DeviceID + "'} WHERE AssocClass=Win32_LogicalDiskToPartition")
			t.Logf("volumes %+v error %v", vs, e)
		}
	}
}
