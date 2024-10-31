package main

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	// flags support

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"

	// logrus
	log "github.com/sirupsen/logrus"
)

func main() {
	hdl := rpc.NewGuestRPCHandler(
		func(req *rpc.JsonRPCRequest) error {
			log.Infof("Request: %s", *req.Method)
			return nil
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
	hdl.SetInput(os.Stdin)
	hdl.SetOutput(os.Stdout)
	hdl.Run()

	// read json from stdin and write to stdout
	chatMsg := openai.ChatCompletionRequest{
		Model: "gpt-4o",
		Messages: []openai.ChatMessage{
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

	req := rpc.NewJsonRPCRequest(openai.HostCallChatCompletion, chatMsg)
	err := req.Send(os.Stdout)
	if err != nil {
		panic(err)
	}

	// send an embeddings request
	embeddingsReq := openai.EmbeddingsRequest{
		Model: "text-embedding-ada-002",
		Input: "The food was delicious and the waiter...",
	}

	req2 := rpc.NewJsonRPCRequest(openai.HostCallEmbeddings, embeddingsReq)
	err = req2.Send(os.Stdout)
	if err != nil {
		panic(err)
	}

	randName := fmt.Sprintf("vdb-%d", time.Now().UnixNano())

	// vector store ops
	req3 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreCreate, payload.VectorStoreCreateRequest{
		Name:       randName,
		Dimentions: 4,
	})
	err = req3.Send(os.Stdout)
	if err != nil {
		panic(err)
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
		err = req3_5.Send(os.Stdout)
		if err != nil {
			panic(err)
		}
	}

	req3_6 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreSearch, payload.VectorStoreSearchRequest{
		VID:    0,
		Vector: []float32{0.2, 0.1, 0.9, 0.7},
		Limit:  1,
	})
	err = req3_6.Send(os.Stdout)
	if err != nil {
		panic(err)
	}

	// delete vector store
	req4 := rpc.NewJsonRPCRequest(payload.HostCallVectorStoreDelete, payload.VectorStoreDeleteRequest{
		VID: 0,
	})
	err = req4.Send(os.Stdout)
	if err != nil {
		panic(err)
	}

	time.Sleep(5 * time.Second)
}
