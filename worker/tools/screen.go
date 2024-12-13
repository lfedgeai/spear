package tools

import (
	"image/png"
	"os"
	"strconv"

	"github.com/kbinani/screenshot"

	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var screenTools = []hccommon.ToolRegistry{
	{
		Name:        "screenshot",
		Description: `Take screenshots of current screens`,
		Params: map[string]hccommon.ToolParam{
			"filename-prefix": {
				Ptype:       "string",
				Description: "Prefix for the filename",
				Required:    true,
			},
		},
		Cb:        "",
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
