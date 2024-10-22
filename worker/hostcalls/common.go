package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type HostCalls struct {
	// map of hostcalls
	HCMap map[string]func(args interface{}) (interface{}, error)
}

func NewHostCalls() *HostCalls {
	return &HostCalls{
		HCMap: make(map[string]func(args interface{}) (interface{}, error)),
	}
}

func (h *HostCalls) RegisterHostCall(hc *HostCall) error {
	name := hc.Name
	handler := hc.Handler
	log.Debugf("Registering hostcall: %s", name)
	if _, ok := h.HCMap[name]; ok {
		return fmt.Errorf("hostcall already registered: %s", name)
	}
	h.HCMap[name] = handler
	return nil
}

func (h *HostCalls) InstallToTask(t task.Task) error {
	log.Debugf("Installing hostcalls to task: %s", t.Name())
	if t == nil {
		return fmt.Errorf("task is nil")
	}

	// get communication channels
	in, out, err := t.CommChannels()
	if err != nil {
		return err
	}

	go func() {
		for msg := range out {
			// process message
			req := &rpc.JsonRPCRequest{}
			if err := req.Unmarshal(msg); err != nil {
				log.Infof("not a valid request: %v", err)
				continue
			}

			log.Debugf("Hostcall received request: %s", *req.Method)

			// process request
			if handler, ok := h.HCMap[*req.Method]; ok {
				result, err := handler(req.Params)
				if err != nil {
					log.Errorf("Error executing hostcall: %v", err)
					// send error response
					resp := req.CreateErrorResponse(1, err.Error(), nil)
					if data, err := resp.Marshal(); err == nil {
						in <- data
					} else {
						log.Errorf("Error marshalling response: %v", err)
					}
				} else {
					// send success response
					resp := req.CreateSuccessResponse(result)
					if data, err := resp.Marshal(); err == nil {
						log.Debugf("Hostcall response: %s", data)
						// output data + "\n"
						in <- append(data, '\n')
					} else {
						log.Errorf("Error marshalling response: %v", err)
					}
				}
			} else {
				log.Errorf("Hostcall not found: %s", *req.Method)
				// send error response
				resp := req.CreateErrorResponse(2, "method not found", nil)
				if data, err := resp.Marshal(); err == nil {
					// output data + "\n"
					in <- append(data, '\n')
				} else {
					log.Errorf("Error marshalling response: %v", err)
				}
			}
		}
	}()

	return nil
}

type HostCall struct {
	Name    string
	Handler func(args interface{}) (interface{}, error)
}
