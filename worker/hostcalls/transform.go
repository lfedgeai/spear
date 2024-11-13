package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/hostcalls/openai"
	log "github.com/sirupsen/logrus"
)

type TransformRegistry struct {
	name        string
	inputTypes  []payload.TransformType
	outputTypes []payload.TransformType
	operations  []payload.TransformOperation
	cb          func(*hostcalls.Caller, interface{}) (interface{}, error)
}

var (
	globalRegisteredTransform = []TransformRegistry{
		{
			name:        "chatgpt",
			inputTypes:  []payload.TransformType{payload.TransformTypeText},
			outputTypes: []payload.TransformType{payload.TransformTypeText},
			operations:  []payload.TransformOperation{payload.TransformOperationLLM},
			cb:          openai.ChatCompletion,
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

func Transform(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
	log.Infof("Executing hostcall \"%s\" with args %v for task %s",
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

	// find the transform registry
	for _, reg := range globalRegisteredTransform {
		if isSubSetTransform(req.InputTypes, reg.inputTypes) &&
			isSubSetTransform(req.OutputTypes, reg.outputTypes) &&
			isSubsetOperation(req.Operations, reg.operations) {
			return reg.cb(caller, req.Params)
		}
	}

	return nil, fmt.Errorf("hostcall \"%s\" not implemented", payload.HostCallTransform)
}
