package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"time"

	// flags support

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"

	// logrus
	log "github.com/sirupsen/logrus"
)

var hdl *rpc.GuestRPCManager
var hostaddr string
var secret string

var input io.Reader
var output io.Writer

// parse arguments
func init() {
	// get hostaddr and secret from environment variables
	hostaddr = os.Getenv("SERVICE_ADDR")
	secret = os.Getenv("SECRET")

	log.Debugf("Connecting to host at %s", hostaddr)
	// create tcp connection to host
	conn, err := net.Dial("tcp", hostaddr)
	if err != nil {
		log.Fatalf("failed to connect to host: %v", err)
	}

	// sending the secret
	// convert secret string to int64
	secretInt, err := strconv.ParseInt(secret, 10, 64)
	if err != nil {
		log.Fatalf("failed to convert secret to int64: %v", err)
	}
	// convert int64 to little endian byte array
	secretBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(secretBytes, uint64(secretInt))
	// write secret to connection
	_, err = conn.Write(secretBytes)
	if err != nil {
		log.Fatalf("failed to write secret to connection: %v", err)
	}

	// create input and output files from connection
	input = conn
	output = conn
}

func main() {
	hdl := rpc.NewGuestRPCManager(
		func(req *rpc.JsonRPCRequest) (*rpc.JsonRPCResponse, error) {
			log.Debugf("Request: %s", *req.Method)
			return rpc.NewJsonRPCResponse(*req.ID, nil), nil
		},
		nil,
	)
	hdl.SetInput(input)
	hdl.SetOutput(output)

	hdl.RegisterIncomingHandler("handle", func(args interface{}) (interface{}, error) {
		log.Infof("Incoming request: %v", args)
		return "ok", nil
	})
	go hdl.Run()

	resp, err := rpc.ChatCompletion(hdl, "gpt-4o", []payload.ChatMessageV2{
		{
			Metadata: map[string]interface{}{
				"role": "system",
			},
			Content: "Hello, how can I help you?",
		},
		{
			Metadata: map[string]interface{}{
				"role": "user",
			},
			Content: "I need help with my computer",
		},
	}, "")
	if err != nil {
		panic(err)
	}
	log.Infof("Response: %v", resp)

	_, err = rpc.Embeddings(hdl, "text-embedding-ada-002", //"bge-large-en-v1.5"
		"The food was delicious and the waiter...")
	if err != nil {
		panic(err)
	}

	randName := fmt.Sprintf("vdb-%d", time.Now().UnixNano())

	// vector store ops
	req3 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreCreate, payload.VectorStoreCreateRequest{
		Name:       randName,
		Dimentions: 4,
	})
	if resp, err := hdl.SendJsonRequest(req3); err != nil {
		panic(err)
	} else {
		log.Infof("Response: %v", resp)
	}

	data := [][]float32{
		{0.05, 0.61, 0.76, 0.74},
		{0.19, 0.81, 0.75, 0.11},
		{0.36, 0.55, 0.47, 0.94},
		{0.18, 0.01, 0.85, 0.80},
		{0.24, 0.18, 0.22, 0.44},
		{0.35, 0.08, 0.11, 0.44},
	}

	for _, v := range data {
		req3_5 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreInsert, payload.VectorStoreInsertRequest{
			VID:    0,
			Vector: v,
			Data:   []byte("test data"),
		})
		if resp, err := hdl.SendJsonRequest(req3_5); err != nil {
			panic(err)
		} else {
			log.Infof("Response: %.1024v", resp)
		}
	}

	req3_6 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreSearch, payload.VectorStoreSearchRequest{
		VID:    0,
		Vector: []float32{0.2, 0.1, 0.9, 0.7},
		Limit:  1,
	})
	if resp, err := hdl.SendJsonRequest(req3_6); err != nil {
		panic(err)
	} else {
		log.Infof("Response: %v", resp)
	}

	// delete vector store
	req4 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreDelete, payload.VectorStoreDeleteRequest{
		VID: 0,
	})
	if resp, err := hdl.SendJsonRequest(req4); err != nil {
		panic(err)
	} else {
		log.Infof("Response: %v", resp)
	}
}
