package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	t "github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type TransformRegistry struct {
	name        string
	inputTypes  []payload.TransformType
	outputTypes []payload.TransformType
	operations  []payload.TransformOperation
	cb          func(*hostcalls.InvocationInfo, interface{}) (interface{}, error)
}

var (
	globalRegisteredTransform = []TransformRegistry{
		{
			name:        "chat_with_tools",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeText},
			operations: []payload.TransformOperation{
				payload.TransformOperationLLM,
				payload.TransformOperationTools,
			},
			cb: ChatCompletionWithTools,
		},
		{
			name:        "chat",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeText},
			operations:  []payload.TransformOperation{payload.TransformOperationLLM},
			cb:          ChatCompletionNoTools,
		},
		{
			name:        "embeddings",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeVector},
			operations:  []payload.TransformOperation{payload.TransformOperationEmbeddings},
			cb:          Embeddings,
		},
		{
			name:        "text-to-speech",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeAudio},
			operations:  []payload.TransformOperation{payload.TransformOperationTextToSpeech},
			cb:          TextToSpeech,
		},
		{
			name:        "speech-to-text",
			inputTypes:  []payload.TransformType{payload.TransformTypeAudio},
			outputTypes: []payload.TransformType{payload.TransformTypeText},
			operations:  []payload.TransformOperation{payload.TransformOperationSpeechToText},
			cb:          SpeechToText,
		},
		{
			name:        "text-to-image",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeImage},
			operations:  []payload.TransformOperation{payload.TransformOperationTextToImage},
			cb:          TextToImage,
		},
	}
)

func isSubSetTransform(a, b []payload.TransformType) bool {
	for _, t1 := range a {
		found := false
		for _, t2 := range b {
			if t1 == t2 {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	return true
}

func isSubsetOperation(a, b []payload.TransformOperation) bool {
	for _, t1 := range a {
		found := false
		for _, t2 := range b {
			if t1 == t2 {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	return true
}

func TransformConfig(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallTransformConfig, args, task.ID())
	// convert args to TransformConfigRequest
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	req := &payload.TransformConfigRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	if req.Reset {
		task.SetVar(t.TVOpenAIBaseURL, nil)
		task.SetVar(t.TVOpenAIAPIKey, nil)
		return &payload.TransformConfigResponse{
			Result: "success",
		}, nil
	}

	if req.BaseURL != "" {
		task.SetVar(t.TVOpenAIBaseURL, req.BaseURL)
	}
	if req.APIKey != "" {
		task.SetVar(t.TVOpenAIAPIKey, req.APIKey)
	}

	return &payload.TransformConfigResponse{
		Result: "success",
	}, nil
}

func Transform(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallTransform, args, task.ID())
	// convert args to TransformRequest
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	req := &payload.TransformRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	var candid *TransformRegistry

	// find the transform registry
	for _, reg := range globalRegisteredTransform {
		if isSubSetTransform(req.InputTypes, reg.inputTypes) &&
			isSubSetTransform(req.OutputTypes, reg.outputTypes) &&
			isSubsetOperation(req.Operations, reg.operations) {
			if candid != nil {
				if len(reg.inputTypes) <= len(candid.inputTypes) &&
					len(reg.outputTypes) <= len(candid.outputTypes) &&
					len(reg.operations) <= len(candid.operations) {
					candid = &reg
				}
			} else {
				candid = &reg
			}
		}
	}

	if candid != nil {
		log.Infof("Using transform registry %s", candid.name)
		res, err := candid.cb(inv, req.Params)
		if err != nil {
			return nil, fmt.Errorf("error calling %s: %v", candid.name, err)
		}

		transResp := &payload.TransformResponse{
			Results: []payload.TransformResult{
				{
					Type: candid.outputTypes[0],
					Data: res,
				},
			},
		}
		return transResp, nil
	}

	return nil, fmt.Errorf("hostcall \"%s\" not implemented", payload.HostCallTransform)
}
