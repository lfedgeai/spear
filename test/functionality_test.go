package test

import (
	"testing"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/spearlet"
)

func TestFunctionality(t *testing.T) {
	// create config
	config := spearlet.NewExecSpearletConfig(true, common.SpearPlatformAddress, []string{}, true)
	w := spearlet.NewSpearlet(config)
	w.Initialize()

	res, err := w.ExecuteTaskByName("pytest-functionality", true, "handle", "")
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	if len(res) > 1024 {
		res = res[:1024] + "..."
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}
