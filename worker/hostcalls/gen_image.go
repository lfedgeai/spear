package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func TextToImage(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai TextToSpeech
	t := *(inv.Task)
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	req := &transform.ImageGenerationRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	req2 := &oai.OpenAIImageGenerationRequest{
		Model:          req.Model,
		Prompt:         req.Prompt,
		ResponseFormat: req.ResponseFormat,
	}
	res, err := oai.OpenAIImageGeneration(oai.EndpointFromTask(t), req2)
	if err != nil {
		return nil, fmt.Errorf("error calling openai TextToImage: %v", err)
	}

	res2 := &transform.ImageGenerationResponse{
		Created: res.Created,
	}
	for _, obj := range res.Data {
		res2.Data = append(res2.Data, transform.ImageObject{
			Url:           obj.Url,
			B64Json:       obj.B64Json,
			RevisedPrompt: obj.RevisedPrompt,
		})
	}

	return res2, nil
}
