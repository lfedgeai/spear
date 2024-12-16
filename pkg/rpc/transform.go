package rpc

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
)

func ChatCompletion(rpcMgr *GuestRPCManager, model string, msgs []payload.ChatMessageV2, tsId string) ([]payload.ChatMessageV2, error) {
	req := &payload.ChatCompletionRequestV2{
		Model:     model,
		Messages:  msgs,
		ToolsetId: tsId,
	}
	resp, err := RPCManagerSendRequest[payload.TransformResponse](rpcMgr, transform.HostCallTransform, payload.TransformRequest{
		InputTypes:  []payload.TransformType{payload.TransformTypeText},
		OutputTypes: []payload.TransformType{payload.TransformTypeText},
		Operations: []payload.TransformOperation{
			payload.TransformOperationLLM,
			payload.TransformOperationTools,
		},
		Params: req,
	})
	if err != nil {
		return nil, fmt.Errorf("error geting result %v", err)
	}

	if len(resp.Results) != 1 {
		return nil, fmt.Errorf("unexpected number of results: %d", len(resp.Results))
	}

	if resp.Results[0].Type != payload.TransformTypeText {
		return nil, fmt.Errorf("unexpected result type: %v", resp.Results[0].Type)
	}

	var chatResp payload.ChatCompletionResponseV2
	if err := utils.InterfaceToType(&chatResp, resp.Results[0].Data); err != nil {
		return nil, fmt.Errorf("error converting response: %v", err)
	}

	return chatResp.Messages, nil
}

func TextToSpeech(rpcMgr *GuestRPCManager, model, voice, input, format string) (*transform.TextToSpeechResponse, error) {
	req := &transform.TextToSpeechRequest{
		Model:  model,
		Voice:  voice,
		Input:  input,
		Format: format,
	}
	resp, err := RPCManagerSendRequest[payload.TransformResponse](rpcMgr, transform.HostCallTransform, payload.TransformRequest{
		InputTypes:  []payload.TransformType{payload.TransformTypeText},
		OutputTypes: []payload.TransformType{payload.TransformTypeAudio},
		Operations: []payload.TransformOperation{
			payload.TransformOperationTextToSpeech,
		},
		Params: req,
	})
	if err != nil {
		return nil, fmt.Errorf("error getting result: %v", err)
	}

	if len(resp.Results) != 1 {
		return nil, fmt.Errorf("unexpected number of results: %d", len(resp.Results))
	}

	if resp.Results[0].Type != payload.TransformTypeAudio {
		return nil, fmt.Errorf("unexpected result type: %v", resp.Results[0].Type)
	}

	var ttsResp transform.TextToSpeechResponse
	if err := utils.InterfaceToType(&ttsResp, resp.Results[0].Data); err != nil {
		return nil, fmt.Errorf("error converting response: %v", err)
	}

	return &ttsResp, nil
}

func Embeddings(rpcMgr *GuestRPCManager, model, input string) ([]float64, error) {
	req := &transform.EmbeddingsRequest{
		Model: model,
		Input: input,
	}
	resp, err := RPCManagerSendRequest[payload.TransformResponse](rpcMgr, transform.HostCallTransform, payload.TransformRequest{
		InputTypes:  []payload.TransformType{payload.TransformTypeText},
		OutputTypes: []payload.TransformType{payload.TransformTypeVector},
		Operations: []payload.TransformOperation{
			payload.TransformOperationEmbeddings,
		},
		Params: req,
	})
	if err != nil {
		return nil, fmt.Errorf("error getting result: %v", err)
	}

	if len(resp.Results) != 1 {
		return nil, fmt.Errorf("unexpected number of results: %d", len(resp.Results))
	}

	if resp.Results[0].Type != payload.TransformTypeVector {
		return nil, fmt.Errorf("unexpected result type: %v", resp.Results[0].Type)
	}

	var embResp transform.EmbeddingsResponse
	if err := utils.InterfaceToType(&embResp, resp.Results[0].Data); err != nil {
		return nil, fmt.Errorf("error converting response: %v", err)
	}

	if len(embResp.Data) != 1 {
		return nil, fmt.Errorf("unexpected number of embeddings: %d", len(embResp.Data))
	}

	return embResp.Data[0].Embedding, nil
}

func TextToImage(rpcMgr *GuestRPCManager, model, prompt, format string) (*transform.ImageGenerationResponse, error) {
	req := &transform.ImageGenerationRequest{
		Model:          model,
		Prompt:         prompt,
		ResponseFormat: format,
	}
	resp, err := RPCManagerSendRequest[payload.TransformResponse](rpcMgr, transform.HostCallTransform, payload.TransformRequest{
		InputTypes:  []payload.TransformType{payload.TransformTypeText},
		OutputTypes: []payload.TransformType{payload.TransformTypeImage},
		Operations: []payload.TransformOperation{
			payload.TransformOperationTextToImage,
		},
		Params: req,
	})
	if err != nil {
		return nil, fmt.Errorf("error getting result: %v", err)
	}

	if len(resp.Results) != 1 {
		return nil, fmt.Errorf("unexpected number of results: %d", len(resp.Results))
	}

	if resp.Results[0].Type != payload.TransformTypeImage {
		return nil, fmt.Errorf("unexpected result type: %v", resp.Results[0].Type)
	}

	var ttiResp transform.ImageGenerationResponse
	if err := utils.InterfaceToType(&ttiResp, resp.Results[0].Data); err != nil {
		return nil, fmt.Errorf("error converting response: %v", err)
	}

	return &ttiResp, nil
}
