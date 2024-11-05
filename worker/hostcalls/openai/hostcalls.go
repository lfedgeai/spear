package openai

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"

	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

func sendBufferData(data *bytes.Buffer, url string) ([]byte, error) {
	// create a https request to url and use data as the request body
	req, err := http.NewRequest("POST", url, data)
	if err != nil {
		return nil, fmt.Errorf("error creating request: %v", err)
	}

	// get api key from environment variable
	apiKey := os.Getenv("OPENAI_API_KEY")
	// set the headers
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", apiKey))
	// send the request
	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("error reading response: %v", err)
	}

	return body, nil
}

func ChatCompletion(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	log.Infof("Executing hostcall \"%s\" with args %v", openai.HostCallChatCompletion, args)
	// verify the type of args is ChatCompletionRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	chatReq := openai.ChatCompletionRequest{}
	err = chatReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("Chat Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/chat/completions and use b as the request body
	res, err := sendBufferData(bytes.NewBuffer(jsonBytes), "https://api.openai.com/v1/chat/completions")
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	respData := openai.ChatCompletionResponse{}
	err = respData.Unmarshal(res)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}

	// return the response
	return respData, nil
}

func Embeddings(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	log.Infof("Executing hostcall \"%s\" with args %v", openai.HostCallEmbeddings, args)
	// verify the type of args is EmbeddingsRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	embeddingsReq := openai.EmbeddingsRequest{}
	err = embeddingsReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Infof("Embeddings Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/embeddings and use b as the request body
	res, err := sendBufferData(bytes.NewBuffer(jsonBytes), "https://api.openai.com/v1/embeddings")
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	respData := openai.EmbeddingsResponse{}
	err = respData.Unmarshal(res)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}

	// return the response
	return respData, nil
}
