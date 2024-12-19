package common

import (
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type ToolId int
type ToolsetId int
type BuiltInToolCbFunc func(inv *InvocationInfo, args interface{}) (interface{}, error)

type ToolParam struct {
	Ptype       string
	Description string
	Required    bool
}

type ToolRegistry struct {
	Name        string
	Description string
	Params      map[string]ToolParam
	Cb          string
	CbBuiltIn   BuiltInToolCbFunc
}

type ToolsetRegistry struct {
	Description     string
	Tools           map[ToolId]ToolRegistry
	WorkloadToolset *WorkloadToolset
	Instance        task.Task
}

var (
	GlobalTaskToolsets = map[task.TaskID]map[ToolsetId]ToolsetRegistry{}
)

var BuiltinTools = []ToolRegistry{}

func GetBuiltinTools() []ToolRegistry {
	return BuiltinTools
}

func RegisterBuiltinTool(tool ToolRegistry) {
	BuiltinTools = append(BuiltinTools, tool)
}

func init() {
	task.RegisterFinaleCallback("toolsets_cleanup", func(task task.Task) {
		log.Infof("Cleaning up toolsets for task [%s]", task.Name())
		for _, toolset := range GlobalTaskToolsets[task.ID()] {
			if toolset.Instance != nil {
				toolset.Instance.Stop()
			}
		}
		delete(GlobalTaskToolsets, task.ID())
	})
}
