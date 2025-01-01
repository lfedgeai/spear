package tools

import (
	"fmt"
	"time"

	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
)

var dtTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "datetime",
		Id:          hccommon.BuiltinToolID_Datetime,
		Description: "Get current date and time, including timezone information",
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn:   datetime,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "sleep",
		Id:          hccommon.BuiltinToolID_Sleep,
		Description: "Sleep for a specified number of seconds",
		Params: map[string]hccommon.ToolParam{
			"seconds": {
				Ptype:       "integer",
				Description: "Number of seconds to sleep",
				Required:    true,
			},
		},
		CbBuiltIn: sleep,
	},
}

func sleep(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	secondsStr := args.(map[string]interface{})["seconds"]
	// it is either float64 or int
	seconds := int(secondsStr.(float64))
	time.Sleep(time.Duration(seconds) * time.Second)
	return fmt.Sprintf("Slept for %d seconds", seconds), nil
}

func datetime(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	return time.Now().Format(time.RFC3339), nil
}

func init() {
	for _, tool := range dtTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
