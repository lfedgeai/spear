package hostcalls

import (
	"encoding/json"
	"fmt"

	"math/rand"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

type MessagePassingRegistry struct {
	name        string
	method      string
	pendingData chan interface{}
	id          uint64
}

var (
	globalRegisteredMessagePassing = map[string]MessagePassingRegistry{}
)

func MessagePassingRegister(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallMessagePassingRegister, args, task.ID())

	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.MessagePassingRegisterRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	globalRegisteredMessagePassing[req.Name] = MessagePassingRegistry{
		name:        req.Name,
		method:      req.Method,
		pendingData: make(chan interface{}),
		id:          rand.Uint64(),
	}

	return &payload.MessagePassingRegisterResponse{
		MsgPassingId: globalRegisteredMessagePassing[req.Name].id,
	}, nil
}

func MessagePassingUnregister(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallMessagePassingUnregister, args, task.ID())

	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.MessagePassingUnregisterRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	found := false

	for k, v := range globalRegisteredMessagePassing {
		if v.id == req.MsgPassingId {
			delete(globalRegisteredMessagePassing, k)
			found = true
			break
		}
	}

	if !found {
		return nil, fmt.Errorf("message passing id not found")
	}

	return &payload.MessagePassingUnregisterResponse{
		MsgPassingId: req.MsgPassingId,
	}, nil
}

func MessagePassingLookup(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallMessagePassingLookup, args, task.ID())

	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.MessagePassingLookupRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	if v, ok := globalRegisteredMessagePassing[req.Name]; ok {
		return &payload.MessagePassingLookupResponse{
			MsgPassingId: v.id,
		}, nil
	}

	return nil, fmt.Errorf("message passing name not found")
}

func MessagePassingSend(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("Executing hostcall \"%s\" with args %v for task %s",
		payload.HostCallMessagePassingSend, args, task.ID())

	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.MessagePassingSendRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	for _, v := range globalRegisteredMessagePassing {
		if v.id == req.MsgPassingId {
			// send without blocking
			select {
			case v.pendingData <- req.Data:
				return &payload.MessagePassingSendResponse{
					MsgPassingId: req.MsgPassingId,
				}, nil
			default:
				return nil, fmt.Errorf("message passing channel full")
			}
		}
	}

	return nil, fmt.Errorf("message passing id not found")
}
