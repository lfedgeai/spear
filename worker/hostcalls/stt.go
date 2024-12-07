package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func SpeechToText(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai SpeechToText
	t := *(inv.Task)
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	req := &transform.SpeechToTextRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	req2 := &oai.OpenAISpeechToTextRequest{
		Model: req.Model,
		Audio: req.Audio,
	}
	res, err := oai.OpenAISpeechToText(oai.EndpointFromTask(t), req2)
	if err != nil {
		return nil, fmt.Errorf("error calling openai SpeechToText: %v", err)
	}

	return res, nil
}
