package common

type WorkloadID [64]byte

type WorkloadType int

const (
	WorkloadTypeUnknown WorkloadType = iota
	WorkloadTypeDocker               // 1
	WorkloadTypeProcess              // 2
	WorkloadTypeDylib                // 3
	WorkloadTypeWasm                 // 4
)

type ToolArgument struct {
	Ptype       string
	Description string
	Required    bool
}

type BaseToolInfo struct {
	Name        string
	Description string
	ID          int
	Params      map[string]ToolArgument
}

type FuncToolInfo struct {
	BaseToolInfo
	CbFn BuiltInToolCbFunc
}

type WorkloadToolInfo struct {
	BaseToolInfo
	CbMethod string
}

type WorkloadToolset struct {
	Name        string
	Description string
	Tools       []WorkloadToolInfo
	Wid         WorkloadID
	Wtype       WorkloadType
}

var globalWorkloadToolsets = map[WorkloadID]*WorkloadToolset{
	{0}: {
		Name:        "py_ocr_tools", // the image name
		Description: "A workload for OCR",
		Tools: []WorkloadToolInfo{
			{
				BaseToolInfo: BaseToolInfo{
					Name:        "ocr",
					Description: "OCR",
					ID:          0,
					Params: map[string]ToolArgument{
						"image": {
							Ptype:       "string",
							Description: "Image data",
							Required:    true,
						},
					},
				},
				CbMethod: "ocr_detect",
			},
		},
		Wid:   WorkloadID{0},
		Wtype: WorkloadTypeDocker,
	},
}

func SearchWorkloadToolsetByID(wid WorkloadID) (*WorkloadToolset, bool) {
	wts, ok := globalWorkloadToolsets[wid]
	return wts, ok
}

func SearchWorkloadToolsetByName(name string) (*WorkloadToolset, bool) {
	for _, wts := range globalWorkloadToolsets {
		if wts.Name == name {
			return wts, true
		}
	}
	return nil, false
}
