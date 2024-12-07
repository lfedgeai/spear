package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func TextToSpeech(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai TextToSpeech
	t := *(inv.Task)

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
	res, err := oai.OpenAITextToSpeech(oai.EndpointFromTask(t), req2)
	if err != nil {
		return nil, fmt.Errorf("error calling openai TextToSpeech: %v", err)
	}

	return res, nil
}
