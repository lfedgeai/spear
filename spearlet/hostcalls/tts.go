package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/spear/proto/transform"
	"github.com/lfedgeai/spear/spearlet/hostcalls/common"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	oai "github.com/lfedgeai/spear/spearlet/hostcalls/openai"
)

func TextToSpeech(inv *hostcalls.InvocationInfo, args *transform.TransformRequest) ([]byte, error) {
	// // right now we just call openai TextToSpeech
	// req := &transform.TextToSpeechRequest{}
	// err := utils.InterfaceToType(&req, args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// req2 := &oai.OpenAITextToSpeechRequest{
	// 	Model:  req.Model,
	// 	Input:  req.Input,
	// 	Voice:  req.Voice,
	// 	Format: req.Format,
	// }
	// ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeTextToSpeech, req2.Model)
	// if len(ep) == 0 {
	// 	return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	// }
	// res, err := oai.OpenAITextToSpeech(ep[0], req2)
	// if err != nil {
	// 	return nil, fmt.Errorf("error calling openai TextToSpeech: %v", err)
	// }

	// return res, nil

	return nil, fmt.Errorf("not implemented")
}

func textToSpeechData(text, model, voice, format string) (string, error) {
	req2 := &oai.OpenAITextToSpeechRequest{
		Model:  model,
		Input:  text,
		Voice:  voice,
		Format: format,
	}
	ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeTextToSpeech, req2.Model)
	if len(ep) == 0 {
		return "", fmt.Errorf("error getting endpoint for model %s", req2.Model)
	}
	res, err := oai.OpenAITextToSpeech(ep[0], req2)
	if err != nil {
		return "", fmt.Errorf("error calling openai TextToSpeech: %v", err)
	}

	return res.EncodedAudio, nil
}
