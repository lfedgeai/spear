package tools

import (
	"fmt"
	"time"

	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var dtTools = []hccommon.ToolRegistry{
	{
		Name:        "datetime",
		Description: "Get current date and time, including timezone information",
		Params:      map[string]hccommon.ToolParam{},
		Cb:          "",
		CbBuiltIn:   datetime,
	},
	{
		Name:        "sleep",
		Description: "Sleep for a specified number of seconds",
		Params: map[string]hccommon.ToolParam{
			"seconds": {
				Ptype:       "integer",
				Description: "Number of seconds to sleep",
				Required:    true,
			},
		},
		Cb:        "",
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
