package main

import (
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net"
	"strconv"
	"time"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"

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
	hdl = rpc.NewGuestRPCManager(nil, nil)
	hdl.SetInput(input)
	hdl.SetOutput(output)

	done := make(chan bool)

	hdl.RegisterIncomingHandler("handle", func(args interface{}) (interface{}, error) {
		defer func() {
			done <- true
		}()
		log.Debugf("Incoming request: %v", args)
		// make sure args is a string
		if str, ok := args.(string); ok {
			resp, err := getTextResponse(str)
			if err != nil {
				return nil, err
			}
			t2sResp, err := text2Speech(resp)
			if err != nil {
				return nil, err
			}
			log.Debugf("Encoded response length in task handle: %d", len(t2sResp.EncodedAudio))
			return t2sResp, nil
		} else {
			return nil, fmt.Errorf("expected string, got %T", args)
		}
	})
	go hdl.Run()

	<-done
	log.Debug("Exiting")
	time.Sleep(5 * time.Second)
}

func getTextResponse(str string) (string, error) {
	chatMsg := payload.ChatCompletionRequest{
		Model: "gpt-4o",
		Messages: []payload.ChatMessage{
			{
				Role:    "user",
				Content: str,
			},
		},
	}
	if resp, err := hdl.SendRequest(openai.HostCallChatCompletion, chatMsg); err != nil {
		return "", err
	} else {
		log.Debugf("Response: %v", resp)
		// marshall and unmarshall to verify the type
		jsonBytes, err := json.Marshal(resp)
		if err != nil {
			return "", fmt.Errorf("error marshalling response: %v", err)
		}
		chatResp := payload.ChatCompletionResponse{}
		err = chatResp.Unmarshal(jsonBytes)
		if err != nil {
			return "", fmt.Errorf("error unmarshalling response: %v", err)
		}
		if len(chatResp.Choices) == 0 {
			return "", fmt.Errorf("no choices returned")
		} else if len(chatResp.Choices) > 1 {
			return "", fmt.Errorf("expected 1 choice, got %d", len(chatResp.Choices))
		}
		return chatResp.Choices[0].Message.Content, nil
	}
}

func text2Speech(str string) (*openai.TextToSpeechResponse, error) {
	t2sReq := openai.TextToSpeechRequest{
		Model:  "tts-1",
		Voice:  "alloy",
		Input:  str,
		Format: "mp3",
	}
	if resp, err := hdl.SendRequest(openai.HostCallTextToSpeech, t2sReq); err != nil {
		return nil, err
	} else {
		// marshall and unmarshall to verify the type
		jsonBytes, err := json.Marshal(resp)
		if err != nil {
			return nil, fmt.Errorf("error marshalling response: %v", err)
		}
		t2sResp := openai.TextToSpeechResponse{}
		err = t2sResp.Unmarshal(jsonBytes)
		if err != nil {
			return nil, fmt.Errorf("error unmarshalling response: %v", err)
		}
		log.Debugf("Encoded Response Len in start.go: %d", len(t2sResp.EncodedAudio))
		return &t2sResp, nil
	}
}
