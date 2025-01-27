package net

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"sync"
	"time"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/lfedgeai/spear/pkg/spear/proto/custom"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	log "github.com/sirupsen/logrus"
)

type RequestHandler func(req *transport.TransportRequest) (*transport.TransportResponse, error)
type ResponseHandler func(resp *transport.TransportResponse) error
type CustomRequestHandler func(req *custom.CustomRequest) (*custom.CustomResponse, error)

type GuestRPCManager struct {
	reqHandler        map[transport.Method]RequestHandler
	customReqHandler  map[string]CustomRequestHandler
	pendingRequests   map[int64]reqCallbackStruct
	pendingRequestsMu sync.RWMutex
	input             io.Reader
	output            io.Writer

	globalIDCounter int64
}

type reqCallbackStruct struct {
	cb        ResponseHandler
	timeStamp time.Time
	autoClear bool
}

const (
	ResponseTimeout = time.Minute * 10 // 10 minutes timeout for requests
)

func RPCManagerSendRequest[T any](rpcMgr *GuestRPCManager, method transport.Method,
	params []byte) (*T, error) {
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

func NewGuestRPCManager() *GuestRPCManager {
	res := &GuestRPCManager{
		reqHandler:        make(map[transport.Method]RequestHandler),
		customReqHandler:  make(map[string]CustomRequestHandler),
		pendingRequests:   make(map[int64]reqCallbackStruct),
		pendingRequestsMu: sync.RWMutex{},
		globalIDCounter:   1,
	}

	res.reqHandler[transport.MethodCustom] =
		func(req *transport.TransportRequest) (*transport.TransportResponse, error) {
			data := req.RequestBytes()
			customReq := custom.GetRootAsCustomRequest(data, 0)
			if customReq == nil {
				return nil, fmt.Errorf("error unmarshalling custom request")
			}
			if hdl, ok := res.customReqHandler[string(customReq.MethodStr())]; ok {
				resp, err := hdl(customReq)
				if err != nil {
					return nil, err
				}
				builder := flatbuffers.NewBuilder(512)
				respOff := builder.CreateByteVector(resp.DataBytes())

				transport.TransportResponseStart(builder)
				transport.TransportResponseAddId(builder, req.Id())
				transport.TransportResponseAddResponse(builder, respOff)
				builder.Finish(transport.TransportResponseEnd(builder))

				data := builder.FinishedBytes()
				transResp := transport.GetRootAsTransportResponse(data, 0)
				if transResp == nil {
					return nil, fmt.Errorf("error unmarshalling response")
				}
				return transResp, nil
			}
			return nil, fmt.Errorf("no handler for custom method %s",
				customReq.MethodStr())
		}

	return res
}

func (g *GuestRPCManager) SetInput(i io.Reader) {
	g.input = i
}

func (g *GuestRPCManager) SetOutput(o io.Writer) {
	g.output = o
}

func (g *GuestRPCManager) SetRequestCallback(id int64, callback ResponseHandler,
	autoClear bool) {
	g.pendingRequestsMu.Lock()
	defer g.pendingRequestsMu.Unlock()
	g.pendingRequests[id] = reqCallbackStruct{
		cb:        callback,
		timeStamp: time.Now(),
		autoClear: autoClear,
	}
}

func (g *GuestRPCManager) ClearRequestCallback(id int64) {
	g.pendingRequestsMu.Lock()
	defer g.pendingRequestsMu.Unlock()
	delete(g.pendingRequests, id)
}

func (g *GuestRPCManager) RegisterIncomingCustomRequestHandler(method string,
	handler CustomRequestHandler) error {
	if _, ok := g.customReqHandler[method]; ok {
		return fmt.Errorf("handler already registered for method %s", method)
	}
	g.customReqHandler[method] = handler
	return nil
}

func (g *GuestRPCManager) RegisterIncomingRequestHandler(method transport.Method,
	handler RequestHandler) error {
	if method == transport.MethodCustom {
		return fmt.Errorf("cannot register handler for custom method")
	}
	if _, ok := g.reqHandler[method]; ok {
		return fmt.Errorf("handler already registered for method %s", method)
	}
	g.reqHandler[method] = handler
	return nil
}

// high level function to send a request
func (g *GuestRPCManager) SendRequest(method transport.Method,
	params []byte) ([]byte, error) {
	builder := flatbuffers.NewBuilder(512)
	paramOff := builder.CreateByteVector(params)

	transport.TransportRequestStart(builder)
	transport.TransportRequestAddMethod(builder, method)
	transport.TransportRequestAddRequest(builder, paramOff)
	builder.Finish(transport.TransportRequestEnd(builder))

	data := builder.FinishedBytes()
	req := transport.GetRootAsTransportRequest(data, 0)
	if req == nil {
		return nil, fmt.Errorf("error unmarshalling request")
	}
	if resp, err := g.SendTransportRequest(req); err != nil {
		return nil, err
	} else {
		return resp.ResponseBytes(), nil
	}
}

// low level function to send a json request
func (g *GuestRPCManager) SendTransportRequest(
	req *transport.TransportRequest) (*transport.TransportResponse, error) {
	if g.output == nil {
		return nil, fmt.Errorf("output file not set")
	}
	builder := flatbuffers.NewBuilder(512)
	reqOffset := builder.CreateByteVector(req.RequestBytes())

	transport.TransportRequestStart(builder)
	transport.TransportRequestAddId(builder, g.globalIDCounter)
	defer func() {
		g.globalIDCounter++
	}()
	transport.TransportRequestAddMethod(builder, req.Method())
	transport.TransportRequestAddRequest(builder, reqOffset)
	off := transport.TransportRequestEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportRequest)
	transport.TransportMessageRawAddData(builder, off)
	builder.Finish(transport.TransportMessageRawEnd(builder))

	data := builder.FinishedBytes()
	dataLen := uint64(len(data))

	ch := make(chan *transport.TransportResponse, 1)
	g.SetRequestCallback(g.globalIDCounter,
		func(resp *transport.TransportResponse) error {
			ch <- resp
			return nil
		}, true)

	// write data length
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, dataLen)
	if _, err := g.output.Write(buf); err != nil {
		return nil, err
	}
	if _, err := g.output.Write(data); err != nil {
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

func (g *GuestRPCManager) SendTransportResponse(id int64,
	resp *transport.TransportResponse) error {
	if g.output == nil {
		return fmt.Errorf("output file not set")
	}
	builder := flatbuffers.NewBuilder(512)
	respOffset := builder.CreateByteVector(resp.ResponseBytes())

	transport.TransportResponseStart(builder)
	transport.TransportResponseAddId(builder, id)
	transport.TransportResponseAddResponse(builder, respOffset)
	transportOff := transport.TransportResponseEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportResponse)
	transport.TransportMessageRawAddData(builder, transportOff)
	builder.Finish(transport.TransportMessageRawEnd(builder))

	data := builder.FinishedBytes()
	dataLen := uint64(len(data))

	// write data length
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, dataLen)
	if _, err := g.output.Write(buf); err != nil {
		return err
	}
	if _, err := g.output.Write(data); err != nil {
		return err
	}
	return nil
}

func (g *GuestRPCManager) sendErrorTransportResponse(id int64,
	err error) error {
	if g.output == nil {
		return fmt.Errorf("output file not set")
	}
	builder := flatbuffers.NewBuilder(512)
	errMsg := builder.CreateString(err.Error())

	transport.TransportResponseStart(builder)
	transport.TransportResponseAddId(builder, id)
	transport.TransportResponseAddCode(builder, -1)
	transport.TransportResponseAddMessage(builder, errMsg)
	transportOff := transport.TransportResponseEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportResponse)
	transport.TransportMessageRawAddData(builder, transportOff)
	builder.Finish(transport.TransportMessageRawEnd(builder))

	data := builder.FinishedBytes()
	dataLen := uint64(len(data))

	// write data length
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, dataLen)
	if _, err := g.output.Write(buf); err != nil {
		return err
	}
	if _, err := g.output.Write(data); err != nil {
		return err
	}
	return nil
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

		req := transport.GetRootAsTransportMessageRaw(data, 0)
		if req == nil {
			log.Errorf("Error unmarshalling request")
			break
		}

		if req.DataType() ==
			transport.TransportMessageRaw_DataTransportRequest {
			// request
			transportReq := &transport.TransportRequest{}
			tbl := flatbuffers.Table{}
			if !req.Data(&tbl) {
				log.Errorf("Error getting data from request")
				break
			}
			transportReq.Init(tbl.Bytes, tbl.Pos)
			if hdl, ok := g.reqHandler[transportReq.Method()]; ok {
				go func() {
					resp, err := hdl(transportReq)
					if err != nil {
						log.Errorf("Error handling request: %v", err)
						if err := g.sendErrorTransportResponse(transportReq.Id(),
							err); err != nil {
							log.Errorf("Error sending error response: %v", err)
						}
					} else {
						log.Debugf("Sending response for method %s",
							transportReq.Method())
						if err := g.SendTransportResponse(transportReq.Id(),
							resp); err != nil {
							log.Errorf("Error sending response: %v", err)
						}
					}
				}()
			}
			// TODO: handle request
		} else if req.DataType() ==
			transport.TransportMessageRaw_DataTransportResponse {
			// response
			transportResp := &transport.TransportResponse{}
			tbl := flatbuffers.Table{}
			if !req.Data(&tbl) {
				log.Errorf("Error getting data from response")
				break
			}
			transportResp.Init(tbl.Bytes, tbl.Pos)
			// check pending requests
			g.pendingRequestsMu.RLock()
			defer g.pendingRequestsMu.RUnlock()
			callback, ok := g.pendingRequests[transportResp.Id()]
			if ok {
				go func() {
					if err := callback.cb(transportResp); err != nil {
						log.Errorf("Error handling response: %v", err)
					}
					if callback.autoClear {
						g.ClearRequestCallback(transportResp.Id())
					}
				}()
			} else {
				log.Errorf("No callback for response id %d", transportResp.Id())
			}
		} else if req.DataType() ==
			transport.TransportMessageRaw_DataTransportSignal {
			// signal
			transportSig := &transport.TransportSignal{}
			tbl := flatbuffers.Table{}
			if !req.Data(&tbl) {
				log.Errorf("Error getting data from signal")
				break
			}
			transportSig.Init(tbl.Bytes, tbl.Pos)
			log.Infof("Got signal: %s. But it is not supported yet.",
				transportSig.Method().String())
		} else {
			log.Errorf("Invalid data type: %v", req.DataType())
			break
		}
	}
}
