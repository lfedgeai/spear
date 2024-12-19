package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
	"github.com/lfedgeai/spear/worker/hostcalls/common"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func SpeechToText(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai SpeechToText
	req := &transform.SpeechToTextRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	req2 := &oai.OpenAISpeechToTextRequest{
		Model: req.Model,
		Audio: req.Audio,
	}
	ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeSpeechToText, req2.Model)
	if len(ep) == 0 {
		return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	}
	res, err := oai.OpenAISpeechToText(ep[0], req2)
	if err != nil {
		return nil, fmt.Errorf("error calling openai SpeechToText: %v", err)
	}

	return res, nil
}
