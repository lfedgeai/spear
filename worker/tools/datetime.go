package tools

import (
	"time"

	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var dtTool = hccommon.ToolRegistry{
	Name:        "datetime",
	Description: "Get current date and time, including timezone information",
	Params:      map[string]hccommon.ToolParam{},
	Cb:          "",
	CbBuiltIn:   datetime,
}

func init() {
	hccommon.RegisterBuiltinTool(dtTool)
}

func datetime(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	return time.Now().Format(time.RFC3339), nil
}
