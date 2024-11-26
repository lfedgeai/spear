package main

import (
	"encoding/binary"
	"flag"
	"fmt"
	"io"
	"net"
	"strconv"
	"time"

	// flags support

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"

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
	flag.StringVar(&hostaddr, "service-addr", "localhost:8080", "host service address")
	flag.StringVar(&secret, "secret", "", "secret for the host service")
	flag.Parse()

	log.Infof("Connecting to host at %s", hostaddr)
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
			log.Infof("Request: %s", *req.Method)
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

	// read json from stdin and write to stdout
	chatMsg := payload.ChatCompletionRequest{
		Model: "gpt-4o",
		Messages: []payload.ChatMessage{
			{
				Role:    "system",
				Content: "Hello, how can I help you?",
			},
			{
				Role:    "user",
				Content: "I need help with my computer",
			},
		},
	}

	if resp, err := hdl.SendRequest(openai.HostCallChatCompletion, chatMsg); err != nil {
		panic(err)
	} else {
		log.Infof("Response: %v", resp)
	}

	// send an embeddings request
	embeddingsReq := openai.EmbeddingsRequest{
		Model: "text-embedding-ada-002",
		Input: "The food was delicious and the waiter...",
	}

	req2 := rpc.NewJsonRPCRequest(openai.HostCallEmbeddings, embeddingsReq)
	if resp, err := hdl.SendJsonRequest(req2); err != nil {
		panic(err)
	} else {
		msg := fmt.Sprintf("Response: %v", resp)
		log.Infof("%.1024s", msg)
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
