package rpc

import (
	"bufio"
	"fmt"
	"os"

	log "github.com/sirupsen/logrus"
)

type RequestHandler func(req *JsonRPCRequest) error
type ResponseHandler func(req *JsonRPCResponse) error

type GuestRPCHandler struct {
	reqHandler  RequestHandler
	respHandler ResponseHandler
	inFile      *os.File
	outFile     *os.File
}

func NewGuestRPCHandler(reqHandler RequestHandler, respHandler ResponseHandler) *GuestRPCHandler {
	return &GuestRPCHandler{
		reqHandler:  reqHandler,
		respHandler: respHandler,
	}
}

func (g *GuestRPCHandler) SetInput(i *os.File) {
	g.inFile = i
}

func (g *GuestRPCHandler) SetOutput(o *os.File) {
	g.outFile = o
}

func (g *GuestRPCHandler) Run() {
	go func() {
		// read from stdin
		reader := bufio.NewReader(g.inFile)

		for {
			// read from stdin
			data, err := reader.ReadBytes('\n')
			if err != nil {
				panic(err)
			}

			if len(data) == 0 {
				log.Infof("Exiting")
				break
			}

			var req JsonRPCRequest
			err = req.Unmarshal([]byte(data))
			if err == nil {
				if err = g.reqHandler(&req); err != nil {
					log.Errorf("Error handling request: %v", err)
				}
				continue
			}

			var resp JsonRPCResponse
			err = resp.Unmarshal([]byte(data))
			if err == nil {
				// response is valid
				if err = g.respHandler(&resp); err != nil {
					log.Errorf("Error handling response: %v", err)
				}
				continue
			}

			panic(fmt.Errorf("invalid request or response"))
		}
	}()
}
