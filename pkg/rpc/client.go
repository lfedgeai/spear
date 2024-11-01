package rpc

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"sync"
	"time"

	log "github.com/sirupsen/logrus"
)

type JsonRPCRequestHandler func(req *JsonRPCRequest) (*JsonRPCResponse, error)
type JsonRPCResponseHandler func(resp *JsonRPCResponse) error

type RequestHandler func(args interface{}) (interface{}, error)

type GuestRPCManager struct {
	reqHandler     map[string]JsonRPCRequestHandler
	reqRawHandler  JsonRPCRequestHandler
	respRawHandler JsonRPCResponseHandler
	inFile         *os.File
	outFile        *os.File
}

var (
	pendingRequests   = map[json.Number]reqCallbackStruct{}
	pendingRequestsMu = sync.RWMutex{}

	globalIDCounter uint64 = 1

	ResponseTimeout = 15 * time.Second
)

type ResquestCallback func(resp *JsonRPCResponse) error
type reqCallbackStruct struct {
	cb        ResquestCallback
	timeStamp time.Time
	autoClear bool
}

func NewGuestRPCManager(reqHandler JsonRPCRequestHandler, respHandler JsonRPCResponseHandler) *GuestRPCManager {
	return &GuestRPCManager{
		reqHandler:     make(map[string]JsonRPCRequestHandler),
		reqRawHandler:  reqHandler,
		respRawHandler: respHandler,
	}
}

func (g *GuestRPCManager) SetInput(i *os.File) {
	g.inFile = i
}

func (g *GuestRPCManager) SetOutput(o *os.File) {
	g.outFile = o
}

func (g *GuestRPCManager) SetRequestCallback(id json.Number, callback ResquestCallback, autoClear bool) {
	pendingRequestsMu.Lock()
	defer pendingRequestsMu.Unlock()
	pendingRequests[id] = reqCallbackStruct{
		cb:        callback,
		timeStamp: time.Now(),
		autoClear: autoClear,
	}
}

func (g *GuestRPCManager) ClearRequestCallback(id json.Number) {
	pendingRequestsMu.Lock()
	defer pendingRequestsMu.Unlock()
	delete(pendingRequests, id)
}

func (g *GuestRPCManager) RegisterIncomingHandler(method string, handler RequestHandler) error {
	if _, ok := g.reqHandler[method]; ok {
		return fmt.Errorf("handler already registered for method %s", method)
	}
	g.reqHandler[method] = func(req *JsonRPCRequest) (*JsonRPCResponse, error) {
		params := req.Params
		result, err := handler(params)
		if err != nil {
			return NewJsonRPCErrorResponse(*req.ID, -1, err.Error(), nil), nil
		}
		return NewJsonRPCResponse(*req.ID, result), nil
	}
	return nil
}

// high level function to send a request
func (g *GuestRPCManager) SendRequest(method string, params interface{}) (interface{}, error) {
	req := NewJsonRPCRequest(method, params)
	resp, err := g.SendJsonRequest(req)
	if err != nil {
		return nil, err
	}
	if resp.Error != nil {
		return nil, fmt.Errorf("error: %v", resp.Error)
	}
	return resp.Result, nil
}

// low level function to send a json request
func (g *GuestRPCManager) SendJsonRequest(req *JsonRPCRequest) (*JsonRPCResponse, error) {
	if g.outFile == nil {
		return nil, fmt.Errorf("output file not set")
	}
	newID := json.Number(fmt.Sprintf("%d", globalIDCounter))
	req.ID = &newID
	globalIDCounter++

	// set callback to unblock
	ch := make(chan *JsonRPCResponse, 1)
	g.SetRequestCallback(*req.ID, func(resp *JsonRPCResponse) error {
		log.Infof("Received response for request %s", *req.ID)
		ch <- resp
		return nil
	}, true)

	if err := req.Send(g.outFile); err != nil {
		return nil, err
	}

	// wait for response
	select {
	case <-time.After(ResponseTimeout):
		return nil, fmt.Errorf("timeout waiting for response")
	case resp := <-ch:
		return resp, nil
	}
}

func (g *GuestRPCManager) sendErrorJsonResponse(id json.Number, err error) error {
	resp := NewJsonRPCErrorResponse(id, -1, err.Error(), nil)
	if g.outFile == nil {
		return fmt.Errorf("output file not set")
	}
	return resp.Send(g.outFile)
}

func (g *GuestRPCManager) Run() {
	// read from stdin
	reader := bufio.NewReader(g.inFile)

	for {
		// read from stdin
		data, err := reader.ReadBytes('\n')
		if err != nil {
			log.Errorf("Error reading from stdin: %v", err)
			continue
		}

		if len(data) == 0 {
			log.Infof("Exiting")
			break
		}

		if string(data) == "\n" {
			continue
		}

		var req JsonRPCRequest
		err = req.Unmarshal([]byte(data))
		if err == nil && g.reqHandler != nil {
			if req.Method == nil {
				log.Errorf("Invalid request: %v", req)
				continue
			}

			// check raw handler
			if g.reqRawHandler != nil {
				resp, err := g.reqRawHandler(&req)
				if err != nil {
					log.Errorf("Error handling request: %v", err)
				}
				if resp != nil {
					if err = resp.Send(g.outFile); err != nil {
						log.Errorf("Error sending response: %v", err)
					}
					continue
				}
			}

			// request is valid
			if hdl, ok := g.reqHandler[*req.Method]; ok {
				if resp, err := hdl(&req); err != nil {
					log.Errorf("Error handling request: %v", err)
					if err = g.sendErrorJsonResponse(*req.ID, err); err != nil {
						log.Errorf("Error sending error response: %v", err)
					}
				} else {
					if err = resp.Send(g.outFile); err != nil {
						log.Errorf("Error sending response: %v", err)
					}
				}
			} else {
				log.Infof("No handler for method %s", *req.Method)
				if err = g.sendErrorJsonResponse(*req.ID, fmt.Errorf("method not found")); err != nil {
					log.Errorf("Error sending error response: %v", err)
				}
			}
			continue
		}

		var resp JsonRPCResponse
		err = resp.Unmarshal([]byte(data))
		if err == nil {
			if g.respRawHandler != nil {
				// response is valid
				if err = g.respRawHandler(&resp); err != nil {
					log.Errorf("Error handling response: %v", err)
				}
			}

			// find the id in the request
			pendingRequestsMu.RLock()
			callback, ok := pendingRequests[*resp.ID]
			pendingRequestsMu.RUnlock()
			if ok {
				if err = callback.cb(&resp); err != nil {
					log.Errorf("Error handling response: %v", err)
				}
				if callback.autoClear {
					g.ClearRequestCallback(*resp.ID)
				}
			}
		}
	}
}
