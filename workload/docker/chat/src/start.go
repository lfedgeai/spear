package main

import (
	"encoding/json"
	"fmt"
	"os"

	// flags support

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"

	// logrus
	log "github.com/sirupsen/logrus"
)

func main() {
	hdl := rpc.NewGuestRPCManager(nil, nil)
	hdl.SetInput(os.Stdin)
	hdl.SetOutput(os.Stdout)

	done := make(chan bool)

	hdl.RegisterIncomingHandler("handle", func(args interface{}) (interface{}, error) {
		defer func() {
			done <- true
		}()
		log.Debugf("Incoming request: %v", args)
		// make sure args is a string
		if str, ok := args.(string); ok {
			chatMsg := openai.ChatCompletionRequest{
				Model: "gpt-4o",
				Messages: []openai.ChatMessage{
					{
						Role:    "user",
						Content: str,
					},
				},
			}
			if resp, err := hdl.SendRequest(openai.HostCallChatCompletion, chatMsg); err != nil {
				return nil, err
			} else {
				log.Debugf("Response: %v", resp)
				// marshall and unmarshall to verify the type
				jsonBytes, err := json.Marshal(resp)
				if err != nil {
					return nil, fmt.Errorf("error marshalling response: %v", err)
				}
				chatResp := openai.ChatCompletionResponse{}
				err = chatResp.Unmarshal(jsonBytes)
				if err != nil {
					return nil, fmt.Errorf("error unmarshalling response: %v", err)
				}
				if len(chatResp.Choices) == 0 {
					return nil, fmt.Errorf("no choices returned")
				} else if len(chatResp.Choices) > 1 {
					return nil, fmt.Errorf("expected 1 choice, got %d", len(chatResp.Choices))
				}
				return chatResp.Choices[0].Message.Content, nil
			}
		} else {
			return nil, fmt.Errorf("expected string, got %T", args)
		}
	})
	go hdl.Run()

	<-done
}
