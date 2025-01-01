package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/spearlet/hostcalls/common"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	oai "github.com/lfedgeai/spear/spearlet/hostcalls/openai"
)

func SpeechToText(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// // right now we just call openai SpeechToText
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }

	// req := &transform.SpeechToTextRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// req2 := &oai.OpenAISpeechToTextRequest{
	// 	Model: req.Model,
	// 	Audio: req.Audio,
	// }
	// ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeSpeechToText, req2.Model)
	// if len(ep) == 0 {
	// 	return nil, fmt.Errorf("error getting endpoint for model %s", req2.Model)
	// }
	// res, err := oai.OpenAISpeechToText(ep[0], req2)
	// if err != nil {
	// 	return nil, fmt.Errorf("error calling openai SpeechToText: %v", err)
	// }

	// return res, nil

	return nil, fmt.Errorf("not implemented")
}

func speechToTextString(audio []byte, model string) (string, error) {
	req2 := &oai.OpenAISpeechToTextRequest{
		Model: model,
		Audio: audio,
	}
	ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeSpeechToText, req2.Model)
	if len(ep) == 0 {
		return "", fmt.Errorf("error getting endpoint for model %s", req2.Model)
	}
	res, err := oai.OpenAISpeechToText(ep[0], req2)
	if err != nil {
		return "", fmt.Errorf("error calling openai SpeechToText: %v", err)
	}

	return res.Text, nil
}
