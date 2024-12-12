package tools

import (
	"fmt"
	"os/exec"

	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var webTools = []hccommon.ToolRegistry{
	{
		Name:        "open_url",
		Description: `Open a URL in the default browser`,
		Params: map[string]hccommon.ToolParam{
			"url": {
				Ptype:       "string",
				Description: "URL to open",
				Required:    true,
			},
		},
		Cb:        "",
		CbBuiltIn: openUrl,
	},
}

func openUrl(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	// use apple script to open URL
	script := `set targetURL to "` + args.(map[string]interface{})["url"].(string) + `"
			tell application "Edge"
				activate
				open location targetURL
			end tell`
	_, err := exec.Command("osascript", "-e", script).Output()
	if err != nil {
		return nil, err
	}
	return fmt.Sprintf("URL %s opened successfully", args.(map[string]interface{})["url"].(string)), nil
}

func init() {
	for _, tool := range webTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
