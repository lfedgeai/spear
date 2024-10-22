package main

import (
	"encoding/json"
	"os"
	"time"

	// flags support
	"flag"

	"github.com/lfedgeai/spear/pkg/openai"
	"github.com/lfedgeai/spear/pkg/rpc"

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

			if len(data) > 1024 {
				log.Infof("Response: %s", data[:1024])
			} else {
				log.Infof("Response: %s", data)
			}

			// resp2 := openai.ChatCompletionResponse{}
			// err = resp2.Unmarshal(data)
			// if err != nil {
			// 	log.Errorf("Error unmarshalling response: %v", err)
			// 	panic(err)
			// }
			// log.Infof("Response Choices: %v", resp2.Choices)

			return nil
		},
	)
	hdl.SetInput(inPipe)
	hdl.SetOutput(outPipe)
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
	b, err := req.Marshal()
	if err != nil {
		panic(err)
	}
	// write b + '\n' to output pipe
	outPipe.Write(append(b, '\n'))

	// send an embeddings request
	embeddingsReq := openai.EmbeddingsRequest{
		Model: "text-embedding-ada-002",
		Input: "The food was delicious and the waiter...",
	}

	req2 := rpc.NewJsonRPCRequest(openai.HostCallEmbeddings, embeddingsReq)
	b2, err := req2.Marshal()
	if err != nil {
		panic(err)
	}
	// write b2 + '\n' to output pipe
	outPipe.Write(append(b2, '\n'))

	time.Sleep(5 * time.Second)
}
