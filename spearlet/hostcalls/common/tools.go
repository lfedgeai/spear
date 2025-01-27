package common

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/spear/proto/tool"
	"github.com/lfedgeai/spear/spearlet/task"
)

type BuiltInToolCbFunc func(inv *InvocationInfo, args interface{}) (interface{}, error)

type ToolParam struct {
	Ptype       string
	Description string
	Required    bool
}

type ToolId uint16
type BuiltinToolID ToolId
type InternalToolID ToolId

type ToolRegistry struct {
	ToolType     ToolType
	Name         string
	Id           BuiltinToolID
	Description  string
	Params       map[string]ToolParam
	CbIdInternal InternalToolID
	CbBuiltIn    BuiltInToolCbFunc
}

type ToolType int

const (
	ToolType_Invalid ToolType = iota
	ToolType_Internal
	ToolType_Builtin
	ToolType_Normal
)

const (
	BuiltinToolID_Invalid            = BuiltinToolID(tool.BuiltinToolIDInvalid)
	BuiltinToolID_Datetime           = BuiltinToolID(tool.BuiltinToolIDDatetime)
	BuiltinToolID_Sleep              = BuiltinToolID(tool.BuiltinToolIDSleep)
	BuiltinToolID_SearchContactEmail = BuiltinToolID(tool.BuiltinToolIDSearchContactEmail)
	// email tools
	BuiltinToolID_ListOpenEmails       = BuiltinToolID(tool.BuiltinToolIDListOpenEmails)
	BuiltinToolID_ComposeEmail         = BuiltinToolID(tool.BuiltinToolIDComposeEmail)
	BuiltinToolID_SendEmailDraftWindow = BuiltinToolID(tool.BuiltinToolIDSendEmailDraftWindow)
	// mouse tools
	BuiltinToolID_MouseRightClick = BuiltinToolID(tool.BuiltinToolIDMouseRightClick)
	BuiltinToolID_MouseLeftClick  = BuiltinToolID(tool.BuiltinToolIDMouseLeftClick)
	// phone tools
	BuiltinToolID_PhoneCall = BuiltinToolID(tool.BuiltinToolIDPhoneCall)
	// screen tools
	BuiltinToolID_FullScreenshot = BuiltinToolID(tool.BuiltinToolIDFullScreenshot)
	// web tools
	BuiltinToolID_OpenURL       = BuiltinToolID(tool.BuiltinToolIDOpenURL)
	BuiltinToolID_ScrollDown    = BuiltinToolID(tool.BuiltinToolIDScrollDown)
	BuiltinToolID_ScrollUp      = BuiltinToolID(tool.BuiltinToolIDScrollUp)
	BuiltinToolID_PageDown      = BuiltinToolID(tool.BuiltinToolIDPageDown)
	BuiltinToolID_PageUp        = BuiltinToolID(tool.BuiltinToolIDPageUp)
	BuiltinToolID_WebScreenshot = BuiltinToolID(tool.BuiltinToolIDWebScreenshot)

	BuiltinToolID_Max = BuiltinToolID(tool.BuiltinToolIDMax)
)

var (
	taskInternalTools = map[task.TaskID][]ToolRegistry{}
	builtinTools      = map[BuiltinToolID]ToolRegistry{}
)

// builtin tools

func RegisterBuiltinTool(tool ToolRegistry) error {
	if _, ok := builtinTools[tool.Id]; ok {
		return fmt.Errorf("duplicate tool registration")
	}
	builtinTools[tool.Id] = tool
	return nil
}

func UnregisterBuiltinTool(id BuiltinToolID) {
	delete(builtinTools, id)
}

func GetBuiltinTool(id BuiltinToolID) (ToolRegistry, bool) {
	tool, ok := builtinTools[id]
	return tool, ok
}

// task internal tools

func RegisterTaskInternalTool(t task.Task, tool ToolRegistry) (InternalToolID, error) {
	if tool.CbIdInternal != InternalToolID(0) {
		return InternalToolID(0),
			fmt.Errorf("the registered tool must not have a callback id set")
	}
	if _, ok := taskInternalTools[t.ID()]; !ok {
		taskInternalTools[t.ID()] = []ToolRegistry{}
	}
	taskInternalTools[t.ID()] = append(taskInternalTools[t.ID()], tool)
	newId := InternalToolID(len(taskInternalTools[t.ID()]) - 1)
	taskInternalTools[t.ID()][newId].CbIdInternal = newId
	return newId, nil
}

func ClearTaskInternalTools(t task.Task) {
	delete(taskInternalTools, t.ID())
}

func GetTaskInternalTool(t task.Task, tid BuiltinToolID) (ToolRegistry, bool) {
	taskTool, ok := taskInternalTools[t.ID()]
	if !ok {
		return ToolRegistry{}, false
	}
	if tid >= BuiltinToolID(len(taskTool)) {
		return ToolRegistry{}, false
	}
	return taskTool[tid], true
}

func GetTaskInternalToolByName(t task.Task, name string) (ToolRegistry, bool) {
	if _, ok := taskInternalTools[t.ID()]; !ok {
		return ToolRegistry{}, false
	}
	for tid, tool := range taskInternalTools[t.ID()] {
		if tool.Name == name {
			return taskInternalTools[t.ID()][tid], true
		}
	}
	return ToolRegistry{}, false
}
