package rpc

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
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
	input          io.Reader
	output         io.Writer
}

type ResquestCallback func(resp *JsonRPCResponse) error
type reqCallbackStruct struct {
	cb        ResquestCallback
	timeStamp time.Time
	autoClear bool
}

var (
	pendingRequests   = map[json.Number]reqCallbackStruct{}
	pendingRequestsMu = sync.RWMutex{}

	globalIDCounter uint64 = 1

	ResponseTimeout = time.Minute * 10 // 10 minutes timeout for requests
)

func RPCManagerSendRequest[T any](rpcMgr *GuestRPCManager, method string, params interface{}) (*T, error) {
	resp, err := rpcMgr.SendRequest(method, params)
	if err != nil {
		return nil, err
	}
	// first marshal to json
	jsonData, err := json.Marshal(resp)
	if err != nil {
		return nil, err
	}
	// then unmarshal to T
	var resp2 T
	err = json.Unmarshal(jsonData, &resp2)
	if err != nil {
		return nil, err
	}
	return &resp2, nil
}

func NewGuestRPCManager(reqHandler JsonRPCRequestHandler, respHandler JsonRPCResponseHandler) *GuestRPCManager {
	return &GuestRPCManager{
		reqHandler:     make(map[string]JsonRPCRequestHandler),
		reqRawHandler:  reqHandler,
		respRawHandler: respHandler,
	}
}

func (g *GuestRPCManager) SetInput(i io.Reader) {
	g.input = i
}

func (g *GuestRPCManager) SetOutput(o io.Writer) {
	g.output = o
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
	if g.output == nil {
		return nil, fmt.Errorf("output file not set")
	}
	newID := json.Number(fmt.Sprintf("%d", globalIDCounter))
	req.ID = &newID
	globalIDCounter++

	// set callback to unblock
	ch := make(chan *JsonRPCResponse, 1)
	g.SetRequestCallback(*req.ID, func(resp *JsonRPCResponse) error {
		log.Debugf("Received response for request %s", *req.ID)
		ch <- resp
		return nil
	}, true)

	if err := req.Send(g.output); err != nil {
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
	if g.output == nil {
		return fmt.Errorf("output file not set")
	}
	return resp.Send(g.output)
}

func (g *GuestRPCManager) Run() {
	// read from stdin
	reader := g.input

	for {
		// read a 64 bit uint
		buf := make([]byte, 8)
		if _, err := reader.Read(buf); err != nil {
			log.Errorf("Error reading from stdin: %v", err)
			continue
		}
		dataLen := binary.LittleEndian.Uint64(buf)

		if dataLen == 0 {
			log.Infof("Exiting")
			break
		}

		log.Debugf("Got message size: %d", dataLen)
		// read dataLen bytes
		data := make([]byte, dataLen)
		if _, err := io.ReadFull(reader, data); err != nil {
			log.Errorf("Error reading from stdin: %v", err)
			continue
		}

		if len(data) == 0 {
			log.Infof("Exiting")
			break
		}

		var req JsonRPCRequest
		err := req.Unmarshal([]byte(data))
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
					if err = resp.Send(g.output); err != nil {
						log.Errorf("Error sending response: %v", err)
					}
					continue
				}
			}

			// request is valid
			if hdl, ok := g.reqHandler[*req.Method]; ok {
				go func() {
					if resp, err := hdl(&req); err != nil {
						log.Errorf("Error handling request: %v", err)
						if err = g.sendErrorJsonResponse(*req.ID, err); err != nil {
							log.Errorf("Error sending error response: %v", err)
						}
					} else {
						log.Debugf("Sending response for method %s", *req.Method)
						if err = resp.Send(g.output); err != nil {
							log.Errorf("Error sending response: %v", err)
						}
					}
				}()
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
				go func() {
					if err = callback.cb(&resp); err != nil {
						log.Errorf("Error handling response: %v", err)
					}
					if callback.autoClear {
						g.ClearRequestCallback(*resp.ID)
					}
				}()
			}
		}
	}
}
