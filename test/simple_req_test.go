package test

import (
	"testing"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/spearlet"
)

func TestLocalPydummy(t *testing.T) {
	// create config
	config := spearlet.NewExecSpearletConfig(true, common.SpearPlatformAddress, []string{}, true)
	w := spearlet.NewSpearlet(config)
	w.Initialize()

	res, err := w.ExecuteTaskByName("pydummy", true, "handle", "")
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}

func TestLocalGenImage(t *testing.T) {
	// create config
	config := spearlet.NewExecSpearletConfig(true, common.SpearPlatformAddress, []string{}, true)
	w := spearlet.NewSpearlet(config)
	w.Initialize()

	res, err := w.ExecuteTaskByName("gen_image", true, "handle", "a red bird.")
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	if len(res) > 1024 {
		res = res[:1024] + "..."
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}
