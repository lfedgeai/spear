package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
	"github.com/lfedgeai/spear/worker/hostcalls/common"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	oai "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

func TextToImage(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// right now we just call openai TextToSpeech

	req := &transform.ImageGenerationRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	req2 := &oai.OpenAIImageGenerationRequest{
		Model:          req.Model,
		Prompt:         req.Prompt,
		ResponseFormat: req.ResponseFormat,
	}
	ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeImageGeneration, req2.Model)
	if len(ep) == 0 {
		return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	}
	res, err := oai.OpenAIImageGeneration(ep[0], req2)
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
