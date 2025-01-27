package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/spear/proto/transform"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

type TransformRegistry struct {
	name        string
	inputTypes  []transform.TransformType
	outputTypes []transform.TransformType
	operations  []transform.TransformOperation
	cb          func(*hostcalls.InvocationInfo, *transform.TransformRequest) ([]byte, error)
}

var (
	globalRegisteredTransform = []TransformRegistry{
		{
			name:        "chat_with_tools",
			inputTypes:  []transform.TransformType{transform.TransformTypeText},
			outputTypes: []transform.TransformType{transform.TransformTypeText},
			operations: []transform.TransformOperation{
				transform.TransformOperationLLM,
				transform.TransformOperationTools,
			},
			cb: ChatCompletionWithTools,
		},
		{
			name:        "chat",
			inputTypes:  []transform.TransformType{transform.TransformTypeText},
			outputTypes: []transform.TransformType{transform.TransformTypeText},
			operations:  []transform.TransformOperation{transform.TransformOperationLLM},
			cb:          ChatCompletionNoTools,
		},
		// 		{
		// 			name:        "embeddings",
		// 			inputTypes:  []payload.TransformType{payload.TransformTypeText},
		// 			outputTypes: []payload.TransformType{payload.TransformTypeVector},
		// 			operations:  []payload.TransformOperation{payload.TransformOperationEmbeddings},
		// 			cb:          Embeddings,
		// 		},
		// 		{
		// 			name:        "text-to-speech",
		// 			inputTypes:  []payload.TransformType{payload.TransformTypeText},
		// 			outputTypes: []payload.TransformType{payload.TransformTypeAudio},
		// 			operations:  []payload.TransformOperation{payload.TransformOperationTextToSpeech},
		// 			cb:          TextToSpeech,
		// 		},
		// 		{
		// 			name:        "speech-to-text",
		// 			inputTypes:  []payload.TransformType{payload.TransformTypeAudio},
		// 			outputTypes: []payload.TransformType{payload.TransformTypeText},
		// 			operations:  []payload.TransformOperation{payload.TransformOperationSpeechToText},
		// 			cb:          SpeechToText,
		// 		},
		// 		{
		// 			name:        "text-to-image",
		// 			inputTypes:  []payload.TransformType{payload.TransformTypeText},
		// 			outputTypes: []payload.TransformType{payload.TransformTypeImage},
		// 			operations:  []payload.TransformOperation{payload.TransformOperationTextToImage},
		// 			cb:          TextToImage,
		// 		},
	}
)

func isSubSetOf[T comparable](a, b []T) bool {
	for _, x := range a {
		found := false
		for _, y := range b {
			if x == y {
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

func TransformConfig(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// task := *(inv.Task)
	// log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
	// 	payload.HostCallTransformConfig, args, task.ID())
	// // convert args to TransformConfigRequest
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }

	// req := &payload.TransformConfigRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// if req.Reset {
	// 	task.SetVar(t.TVTest, nil)
	// 	return &payload.TransformConfigResponse{
	// 		Result: "success",
	// 	}, nil
	// }

	// if req.Test != "" {
	// 	task.SetVar(t.TVTest, req.Test)
	// }

	// return &payload.TransformConfigResponse{
	// 	Result: "success",
	// }, nil
	return nil, fmt.Errorf("hostcall   not implemented")
}

func Transform(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	req := transform.GetRootAsTransformRequest(args, 0)
	if req == nil {
		return nil, fmt.Errorf("could not get TransformRequest")
	}

	var candid *TransformRegistry

	inputTypes := make([]transform.TransformType, req.InputTypesLength())
	for i := 0; i < req.InputTypesLength(); i++ {
		inputTypes[i] = req.InputTypes(i)
	}
	outputTypes := make([]transform.TransformType, req.OutputTypesLength())
	for i := 0; i < req.OutputTypesLength(); i++ {
		outputTypes[i] = req.OutputTypes(i)
	}
	operations := make([]transform.TransformOperation, req.OperationsLength())
	for i := 0; i < req.OperationsLength(); i++ {
		operations[i] = req.Operations(i)
	}
	// find the transform registry
	for _, reg := range globalRegisteredTransform {
		if isSubSetOf(inputTypes, reg.inputTypes) &&
			isSubSetOf(outputTypes, reg.outputTypes) &&
			isSubSetOf(operations, reg.operations) {
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
		res, err := candid.cb(inv, req)
		if err != nil {
			return nil, fmt.Errorf("error calling %s: %v", candid.name, err)
		}

		log.Debugf("Transform result: %+v", res)
		return res, nil
	}

	return nil, fmt.Errorf("hostcall \"%v\" not implemented", transport.MethodTransform)
}
