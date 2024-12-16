package common

import (
	"github.com/lfedgeai/spear/worker/task"
)

type ToolId string
type ToolsetId string
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
	Description string
	ToolsIds    []ToolId
}

var (
	GlobalTaskTools    = map[task.TaskID]map[ToolId]ToolRegistry{}
	GlobalTaskToolsets = map[task.TaskID]map[ToolsetId]ToolsetRegistry{}
)

var BuiltinTools = []ToolRegistry{}

func GetBuiltinTools() []ToolRegistry {
	return BuiltinTools
}

func RegisterBuiltinTool(tool ToolRegistry) {
	BuiltinTools = append(BuiltinTools, tool)
}
