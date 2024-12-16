package main

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	// flags support
	"flag"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"

	// logrus
	log "github.com/sirupsen/logrus"
)

// input and output flags using -i/-o
var input string
var output string

func init() {
	flag.StringVar(&input, "i", "", "input file")
	flag.StringVar(&output, "o", "", "output file")
	flag.Parse()
}

func main() {
	// open input pipe and output pipe
	inPipe, err := os.OpenFile(input, os.O_RDONLY, os.ModeNamedPipe)
	if err != nil {
		panic(err)
	}
	outPipe, err := os.OpenFile(output, os.O_WRONLY, os.ModeNamedPipe)
	if err != nil {
		panic(err)
	}

	hdl := rpc.NewGuestRPCManager(
		func(req *rpc.JsonRPCRequest) (*rpc.JsonRPCResponse, error) {
			log.Infof("Request: %s", *req.Method)
			return rpc.NewJsonRPCResponse(*req.ID, nil), nil
		},
		func(resp *rpc.JsonRPCResponse) error {
			log.Infof("Response: %s", resp.Result)

			// convert resp.Result to buffer
			data, err := json.Marshal(resp.Result)
			if err != nil {
				log.Errorf("Error marshalling response: %v", err)
				panic(err)
			}

			if len(data) > 2048 {
				log.Infof("Response: %s", data[:2048])
			} else {
				log.Infof("Response: %s", data)
			}

			return nil
		},
	)
	hdl.SetInput(inPipe)
	hdl.SetOutput(outPipe)
	go hdl.Run()

	// // send an embeddings request
	// embeddingsReq := transform.EmbeddingsRequest{
	// 	Model: "text-embedding-ada-002",
	// 	Input: "The food was delicious and the waiter...",
	// }

	// req2 := rpc.NewJsonRPCRequest(transform.HostCallEmbeddings, embeddingsReq)
	// err = req2.Send(outPipe)
	// if err != nil {
	// 	panic(err)
	// }

	randName := fmt.Sprintf("vdb-%d", time.Now().UnixNano())

	// vector store ops
	req3 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreCreate, payload.VectorStoreCreateRequest{
		Name: randName,
	})
	err = req3.Send(outPipe)
	if err != nil {
		panic(err)
	}

	// delete vector store
	req4 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreDelete, payload.VectorStoreDeleteRequest{
		VID: 0,
	})
	err = req4.Send(outPipe)
	if err != nil {
		panic(err)
	}

	time.Sleep(5 * time.Second)
}
