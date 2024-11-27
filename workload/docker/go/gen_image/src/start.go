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
			resp, err := generateImage(str)
			if err != nil {
				log.Errorf("failed to generate image: %v", err)
				return nil, err
			}
			log.Debugf("Generated image: %v", resp)
			return resp, nil
		} else {
			return nil, fmt.Errorf("expected string, got %T", args)
		}
	})
	go hdl.Run()

	<-done
	log.Debug("Exiting")
	time.Sleep(5 * time.Second)
}

func generateImage(str string) (*openai.ImageGenerationResponse, error) {
	imgGenReq := openai.ImageGenerationRequest{
		Model:          "dall-e-3",
		Prompt:         str,
		ResponseFormat: "b64_json",
	}
	rawResp, err := hdl.SendRequest(openai.HostCallImageGeneration, imgGenReq)
	if err != nil {
		return nil, err
	}

	// marshall and unmarshall the response
	tmp, err := json.Marshal(rawResp)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response: %v", err)
	}

	var resp openai.ImageGenerationResponse

	if err := json.Unmarshal(tmp, &resp); err != nil {
		return nil, fmt.Errorf("failed to unmarshal response: %v", err)
	}
	if len(resp.Data) != 1 {
		return nil, fmt.Errorf("expected 1 image, got %d", len(resp.Data))
	}
	return &resp, nil
}
