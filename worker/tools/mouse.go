package tools

import (
	"time"

	"github.com/go-vgo/robotgo"
	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var mouseTools = []hccommon.ToolRegistry{
	{
		Name:        "mouse_right_click",
		Description: `Right click the mouse at the current location.`,
		Params:      map[string]hccommon.ToolParam{},
		Cb:          "",
		CbBuiltIn: func(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
			robotgo.Toggle("right")
			time.Sleep(300 * time.Millisecond)
			robotgo.Toggle("right", "up")
			return "Right click successful", nil
		},
	},
	{
		Name:        "mouse_left_click",
		Description: `Left click the mouse at the current location.`,
		Params:      map[string]hccommon.ToolParam{},
		Cb:          "",
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
