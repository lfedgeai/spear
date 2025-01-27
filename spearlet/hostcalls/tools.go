package hostcalls

import (
	"fmt"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/lfedgeai/spear/pkg/spear/proto/tool"
	hcommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

func NewInternalTool(inv *hcommon.InvocationInfo,
	args []byte) ([]byte, error) {
	req := tool.GetRootAsInternalToolCreateRequest(args, 0)
	if req == nil {
		return nil, fmt.Errorf("could not get InternalToolCreateRequest")
	}

	toolreg := hcommon.ToolRegistry{
		Name:        string(req.Name()),
		Description: string(req.Description()),
		Params:      make(map[string]hcommon.ToolParam),
	}

	for i := 0; i < req.ParamsLength(); i++ {
		paramSpec := tool.InternalToolCreateParamSpec{}
		if !req.Params(&paramSpec, i) {
			return nil, fmt.Errorf("could not get param spec")
		}

		toolreg.Params[string(paramSpec.Name())] = hcommon.ToolParam{
			Ptype:       string(paramSpec.Type()),
			Description: string(paramSpec.Description()),
			Required:    paramSpec.Required(),
		}
	}

	log.Infof("Registering internal tool %+v", toolreg)

	newId, err := hcommon.RegisterTaskInternalTool(inv.Task, toolreg)
	if err != nil {
		return nil, err
	}

	builder := flatbuffers.NewBuilder(0)
	tool.InternalToolCreateResponseStart(builder)
	tool.InternalToolCreateResponseAddToolId(builder, int64(newId))
	builder.Finish(tool.InternalToolCreateResponseEnd(builder))

	return builder.FinishedBytes(), nil
}
