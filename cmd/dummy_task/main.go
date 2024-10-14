package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/lfedgeai/spear/pkg/openai"
	"github.com/lfedgeai/spear/pkg/rpc"
)

func main() {

	done := make(chan struct{})

	hdl := rpc.NewGuestRPCHandler(
		func(req *rpc.JsonRPCRequest) error {
			os.Stderr.Write([]byte(fmt.Sprintf("Request: %s\n", *req.Method)))
			return nil
		},
		func(resp *rpc.JsonRPCResponse) error {
			os.Stderr.Write([]byte(fmt.Sprintf("Response: %s\n", resp.Result)))

			// convert resp.Result to buffer
			data, err := json.Marshal(resp.Result)
			if err != nil {
				os.Stderr.Write([]byte(fmt.Sprintf("Error: %v\n", err)))
				panic(err)
			}

			resp2 := openai.ChatCompletionResponse{}
			err = resp2.Unmarshal(data)
			if err != nil {
				os.Stderr.Write([]byte(fmt.Sprintf("Error: %v\n", err)))
				panic(err)
			}
			os.Stderr.Write([]byte(fmt.Sprintf("Response Choices: %v\n", resp2.Choices)))

			// close done channel
			close(done)
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

	req := rpc.NewJsonRPCRequest("chat.completion", chatMsg)
	b, err := req.Marshal()
	if err != nil {
		panic(err)
	}
	os.Stdout.Write(b)
	os.Stdout.Write([]byte("\n"))

	<-done
}
