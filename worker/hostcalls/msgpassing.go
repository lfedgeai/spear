package hostcalls

import (
	"fmt"

	"math/rand"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/utils"
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

	req := payload.MessagePassingRegisterRequest{}
	if err := utils.InterfaceToType(&req, args); err != nil {
		return nil, err
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

	req := payload.MessagePassingUnregisterRequest{}
	if err := utils.InterfaceToType(&req, args); err != nil {
		return nil, err
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

	req := payload.MessagePassingLookupRequest{}
	if err := utils.InterfaceToType(&req, args); err != nil {
		return nil, err
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

	req := payload.MessagePassingSendRequest{}
	if err := utils.InterfaceToType(&req, args); err != nil {
		return nil, err
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
