package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"

	"github.com/lfedgeai/spear/pkg/openai"
	"github.com/lfedgeai/spear/pkg/rpc"
)

func main() {

	done := make(chan struct{})

	go func() {
		// read from stdin
		reader := bufio.NewReader(os.Stdin)

		for {
			// read from stdin
			data, err := reader.ReadBytes('\n')
			if err != nil {
				panic(err)
			}

			os.Stderr.Write([]byte(fmt.Sprintf("Raw Input For Task: %s\n", data)))

			var req rpc.JsonRPCRequest
			err = req.Unmarshal([]byte(data))
			if err == nil {
				// request is valid

				// TODO:

				os.Stderr.Write([]byte(fmt.Sprintf("Request: %s\n", *req.Method)))
				continue
			}

			var resp rpc.JsonRPCResponse
			err = resp.Unmarshal([]byte(data))
			if err == nil {
				// response is valid

				// print to stderr
				os.Stderr.Write([]byte(fmt.Sprintf("Raw Response: %s\n", resp.Result)))

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
				break
			} else {
				os.Stderr.Write([]byte(fmt.Sprintf("Error: %v\n", err)))
				panic(err)
			}
		}
	}()

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
