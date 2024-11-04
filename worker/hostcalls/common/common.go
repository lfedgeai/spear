package hostcalls

import (
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type HostCall struct {
	Name    string
	Handler HostCallHandler
}

type Caller struct {
	Task *task.Task
}

type RespChanData struct {
	Resp   *rpc.JsonRPCResponse
	Caller *Caller
}

type ReqChanData struct {
	Req    *rpc.JsonRPCRequest
	Caller *Caller
}

// communication manager for hostcalls and guest responses
type CommunicationManager struct {
	respCh chan *RespChanData
	reqCh  chan *ReqChanData
	outCh  chan task.Message

	pendingRequests   map[json.Number]*requestCallback
	pendingRequestsMu sync.RWMutex
}

type HostCallHandler func(caller *Caller, args interface{}) (interface{}, error)

type HostCalls struct {
	// map of hostcalls
	HCMap   map[string]HostCallHandler
	CommMgr *CommunicationManager
}

func NewHostCalls(commMgr *CommunicationManager) *HostCalls {
	return &HostCalls{
		HCMap:   make(map[string]HostCallHandler),
		CommMgr: commMgr,
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

func (h *HostCalls) Run() {
	for {
		entry := h.CommMgr.GetIncomingRequest()
		req := entry.Req
		caller := entry.Caller
		if handler, ok := h.HCMap[*req.Method]; ok {
			result, err := handler(caller, req.Params)
			if err != nil {
				log.Errorf("Error executing hostcall: %v", err)
				// send error response
				resp := req.CreateErrorResponse(1, err.Error(), nil)
				if err := h.CommMgr.SendOutgoingResponse(resp); err != nil {
					log.Errorf("Error sending response: %v", err)
				}
			} else {
				// send success response
				resp := req.CreateSuccessResponse(result)
				if err := h.CommMgr.SendOutgoingResponse(resp); err != nil {
					log.Errorf("Error sending response: %v", err)
				}
			}
		} else {
			log.Errorf("Hostcall not found: %s", *req.Method)
			// send error response
			resp := req.CreateErrorResponse(2, "method not found", nil)
			if err := h.CommMgr.SendOutgoingResponse(resp); err != nil {
				log.Errorf("Error sending response: %v", err)
			}
		}
	}
}

func NewCommunicationManager() *CommunicationManager {
	return &CommunicationManager{
		respCh: make(chan *RespChanData),
		reqCh:  make(chan *ReqChanData),
		outCh:  nil,

		pendingRequests:   make(map[json.Number]*requestCallback),
		pendingRequestsMu: sync.RWMutex{},
	}
}

func (c *CommunicationManager) InstallToTask(t task.Task) error {
	if t == nil {
		log.Errorf("task is nil")
		return fmt.Errorf("task is nil")
	}

	// check in and out channel
	in, out, err := t.CommChannels()
	if err != nil {
		log.Errorf("Error getting communication channels: %v", err)
		return err
	}

	c.outCh = in

	go func() {
		caller := Caller{
			Task: &t,
		}
		for msg := range out {
			// process message
			if string(msg) == "\n" {
				continue
			}

			req := &rpc.JsonRPCRequest{}
			if err := req.Unmarshal(msg); err == nil {
				log.Debugf("Hostcall received request: %s", *req.Method)
				c.reqCh <- &ReqChanData{
					Req:    req,
					Caller: &caller,
				}
			} else {
				resp := &rpc.JsonRPCResponse{}
				if err := resp.Unmarshal(msg); err == nil {
					log.Debugf("Guest response received: %s", *resp.ID)

					// check if it is response to a pending request
					c.pendingRequestsMu.RLock()
					entry, ok := c.pendingRequests[*resp.ID]
					c.pendingRequestsMu.RUnlock()
					if ok {
						cb := entry.cb
						if err := cb(resp); err != nil {
							log.Errorf("Error handling response: %v", err)
						}
						if entry.autoClear {
							c.pendingRequestsMu.Lock()
							delete(c.pendingRequests, *resp.ID)
							c.pendingRequestsMu.Unlock()
						}

						continue
					}

					c.respCh <- &RespChanData{
						Resp:   resp,
						Caller: &caller,
					}
				} else {
					log.Errorf("Invalid request: %v", err)
					continue
				}
			}

		}
	}()

	return nil
}

func (c *CommunicationManager) GetIncomingRequest() *ReqChanData {
	return <-c.reqCh
}

func (c *CommunicationManager) GetIncomingResponse() *RespChanData {
	return <-c.respCh
}

func (c *CommunicationManager) SendOutgoingResponse(resp *rpc.JsonRPCResponse) error {
	if data, err := resp.Marshal(); err == nil {
		c.outCh <- append(data, '\n')
		return nil
	}
	return fmt.Errorf("error marshalling response")
}

type ResquestCallback func(resp *rpc.JsonRPCResponse) error

type requestCallback struct {
	cb        ResquestCallback
	autoClear bool
	ts        time.Time
}

func (c *CommunicationManager) SendOutgoingRequestCallback(req *rpc.JsonRPCRequest, cb func(*rpc.JsonRPCResponse) error) error {
	if data, err := req.Marshal(); err == nil {
		c.outCh <- append(data, '\n')

		// add to pending requests
		c.pendingRequestsMu.Lock()
		c.pendingRequests[*req.ID] = &requestCallback{
			cb:        cb,
			autoClear: true,
			ts:        time.Now(),
		}
		c.pendingRequestsMu.Unlock()
		return nil
	}
	return fmt.Errorf("error marshalling request")
}

func (c *CommunicationManager) SendOutgoingRequest(req *rpc.JsonRPCRequest) (*rpc.JsonRPCResponse, error) {
	ch := make(chan *rpc.JsonRPCResponse, 1)
	if err := c.SendOutgoingRequestCallback(req, func(resp *rpc.JsonRPCResponse) error {
		ch <- resp
		return nil
	}); err != nil {
		return nil, err
	}

	select {
	case resp := <-ch:
		return resp, nil
	case <-time.After(rpc.ResponseTimeout):
		return nil, fmt.Errorf("timeout")
	}
}
