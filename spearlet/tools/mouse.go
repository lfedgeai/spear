package tools

import (
	"time"

	"github.com/go-vgo/robotgo"
	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
)

var mouseTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "mouse_right_click",
		Id:          hccommon.BuiltinToolID_MouseRightClick,
		Description: `Right click the mouse at the current location.`,
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn: func(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
			robotgo.Toggle("right")
			time.Sleep(300 * time.Millisecond)
			robotgo.Toggle("right", "up")
			return "Right click successful", nil
		},
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "mouse_left_click",
		Id:          hccommon.BuiltinToolID_MouseLeftClick,
		Description: `Left click the mouse at the current location.`,
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn: func(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
			robotgo.Toggle("left")
			time.Sleep(300 * time.Millisecond)
			robotgo.Toggle("left", "up")
			return "Left click successful", nil
		},
	},
}

func init() {
	for _, tool := range mouseTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
