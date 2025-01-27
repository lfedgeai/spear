package tools

import (
	"image/png"
	"os"
	"strconv"

	"github.com/kbinani/screenshot"

	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
)

var screenTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "full_screenshot",
		Id:          hccommon.BuiltinToolID_FullScreenshot,
		Description: `Take screenshots of everything on all screens, and save them to files`,
		Params: map[string]hccommon.ToolParam{
			"filename-prefix": {
				Ptype:       "string",
				Description: "Prefix for the filename",
				Required:    true,
			},
		},
		CbBuiltIn: screenshotCall,
	},
}

func screenshotCall(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	for i := range screenshot.NumActiveDisplays() {
		bound := screenshot.GetDisplayBounds(i)
		img, err := screenshot.CaptureRect(bound)
		if err != nil {
			return nil, err
		}
		filename := args.(map[string]interface{})["filename-prefix"].(string) + "_" + strconv.Itoa(i) + ".png"
		file, err := os.Create(filename)
		if err != nil {
			return nil, err
		}
		defer file.Close()
		err = png.Encode(file, img)
		if err != nil {
			return nil, err
		}
	}
	return "Screenshots taken successfully for all screens", nil
}

func init() {
	for _, tool := range screenTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
