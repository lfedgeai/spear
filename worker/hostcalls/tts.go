package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
	"github.com/lfedgeai/spear/worker/hostcalls/common"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func TextToSpeech(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai TextToSpeech
	req := &transform.TextToSpeechRequest{}
	err := utils.InterfaceToType(&req, args)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	req2 := &oai.OpenAITextToSpeechRequest{
		Model:  req.Model,
		Input:  req.Input,
		Voice:  req.Voice,
		Format: req.Format,
	}
	ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeTextToSpeech, req2.Model)
	if len(ep) == 0 {
		return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	}
	res, err := oai.OpenAITextToSpeech(ep[0], req2)
	if err != nil {
		return nil, fmt.Errorf("error calling openai TextToSpeech: %v", err)
	}

	return res, nil
}
