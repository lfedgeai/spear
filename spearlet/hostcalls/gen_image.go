package hostcalls

import (
	"fmt"

	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
)

func TextToImage(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// // right now we just call openai TextToSpeech
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }

	// req := &transform.ImageGenerationRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// req2 := &oai.OpenAIImageGenerationRequest{
	// 	Model:          req.Model,
	// 	Prompt:         req.Prompt,
	// 	ResponseFormat: req.ResponseFormat,
	// }
	// ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeImageGeneration, req2.Model)
	// if len(ep) == 0 {
	// 	return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	// }
	// res, err := oai.OpenAIImageGeneration(ep[0], req2)
	// if err != nil {
	// 	return nil, fmt.Errorf("error calling openai TextToImage: %v", err)
	// }

	// res2 := &transform.ImageGenerationResponse{
	// 	Created: res.Created,
	// }
	// for _, obj := range res.Data {
	// 	res2.Data = append(res2.Data, transform.ImageObject{
	// 		Url:           obj.Url,
	// 		B64Json:       obj.B64Json,
	// 		RevisedPrompt: obj.RevisedPrompt,
	// 	})
	// }

	// return res2, nil

	return nil, fmt.Errorf("not implemented")
}
