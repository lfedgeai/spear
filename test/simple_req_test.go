package test

import (
	"testing"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/spearlet"
	"github.com/lfedgeai/spear/spearlet/task"
)

func TestLocalPydummy(t *testing.T) {
	// create config
	config := spearlet.NewExecSpearletConfig(true, common.SpearPlatformAddress,
		[]string{}, true)
	w := spearlet.NewSpearlet(config)
	w.Initialize()

	res, _, err := w.RunTask(-1, "pydummy", task.TaskTypeDocker, "handle", "", nil,
		true, true)
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}

func TestLocalGenImage(t *testing.T) {
	// create config
	config := spearlet.NewExecSpearletConfig(true, common.SpearPlatformAddress,
		[]string{}, true)
	w := spearlet.NewSpearlet(config)
	w.Initialize()

	res, _, err := w.RunTask(-1, "gen_image", task.TaskTypeDocker, "handle",
		"a red bird", nil, true, true)
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	if len(res) > 1024 {
		res = res[:1024] + "..."
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}
