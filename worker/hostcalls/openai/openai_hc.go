package openai

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"

	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

type OpenAIChatToolCallFunction struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}

type OpenAIChatToolCall struct {
	Id       string                     `json:"id"`
	Type     string                     `json:"type"`
	Function OpenAIChatToolCallFunction `json:"function"`
}

type OpenAIChatMessage struct {
	Role       string               `json:"role"`
	Content    string               `json:"content"`
	ToolCalls  []OpenAIChatToolCall `json:"tool_calls"`
	ToolCallId string               `json:"tool_call_id"`
}

type OpenAIChatCompletionResponse struct {
	Id      string             `json:"id"`
	Model   string             `json:"model"`
	Choices []OpenAIChatChoice `json:"choices"`
}

type OpenAIChatChoice struct {
	Message OpenAIChatMessage `json:"message"`
	Index   json.Number       `json:"index"`
	Reason  string            `json:"finish_reason"`
}

type OpenAIChatToolParameterProperty struct {
	Type        string `json:"type"`
	Description string `json:"description"`
}

type OpenAIChatToolParameter struct {
	Type                 string                                     `json:"type"`
	Required             []string                                   `json:"required"`
	AdditionalProperties bool                                       `json:"additionalProperties"`
	Properties           map[string]OpenAIChatToolParameterProperty `json:"properties"`
}

type OpenAIChatToolFunction struct {
	Type string                    `json:"type"`
	Func OpenAIChatToolFunctionSub `json:"function"`
}

type OpenAIChatToolFunctionSub struct {
	Name        string                  `json:"name"`
	Description string                  `json:"description"`
	Parameters  OpenAIChatToolParameter `json:"parameters"`
}

type OpenAIChatCompletionRequest struct {
	Messages []OpenAIChatMessage      `json:"messages"`
	Model    string                   `json:"model"`
	Tools    []OpenAIChatToolFunction `json:"tools"`
}

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

func OpenAIChatCompletion(chatReq *OpenAIChatCompletionRequest) (*OpenAIChatCompletionResponse, error) {
	jsonBytes, err := json.Marshal(chatReq)
	if err != nil {
		return nil, fmt.Errorf("error marshalling OpenAIChatCompletionRequest: %v", err)
	}

	// log.Debugf("Chat Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/chat/completions and use b as the request body
	res, err := sendBufferData(bytes.NewBuffer(jsonBytes), "https://api.openai.com/v1/chat/completions")
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	// log.Debugf("OpenAI Response: %s", string(res))
	respData := OpenAIChatCompletionResponse{}
	err = json.Unmarshal(res, &respData)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}

	if respData.Id == "" {
		var tmpMap map[string]map[string]string
		err = json.Unmarshal(res, &tmpMap)
		if err != nil {
			return nil, fmt.Errorf("error unmarshalling response: %v", err)
		}
		if tmpMap["error"] != nil {
			return nil, fmt.Errorf("error from OpenAI: %v", tmpMap["error"])
		}
	}

	// return the response
	return &respData, nil
}

func OpenAIChatCompletion_Resp_NeedToolCall(chatResp *OpenAIChatCompletionResponse) bool {
	for _, choice := range chatResp.Choices {
		if len(choice.Message.ToolCalls) > 0 {
			return true
		}
	}
	return false
}

func OpenAIChatCompletion_Resp_IterateToolCalls(chatResp *OpenAIChatCompletionResponse, fn func(funcName string, args interface{}) error) error {
	for _, choice := range chatResp.Choices {
		for _, toolCall := range choice.Message.ToolCalls {
			argsStr := toolCall.Function.Arguments
			// use json to unmarshal the arguments to interface{}
			if argsStr != "" {
				var args interface{}
				err := json.Unmarshal([]byte(argsStr), &args)
				if err != nil {
					return fmt.Errorf("error unmarshalling tool call arguments: %v", err)
				}

				err = fn(toolCall.Function.Name, args)
				if err != nil {
					return err
				}
			} else {
				err := fn(toolCall.Function.Name, nil)
				if err != nil {
					return err
				}
			}
		}
	}
	return nil
}

func Embeddings(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
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

func TextToSpeech(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	log.Infof("Executing hostcall \"%s\" with args %v", openai.HostCallTextToSpeech, args)
	// verify the type of args is TextToSpeechRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	t2sReq := openai.TextToSpeechRequest{}
	err = t2sReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("TextToSpeech Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/text2speech and use b as the request body
	res, err := sendBufferData(bytes.NewBuffer(jsonBytes), "https://api.openai.com/v1/audio/speech")
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	log.Debugf("OpenAI Response Len: %d", len(res))

	respData := openai.TextToSpeechResponse{}
	// base64 encode the audio data
	encodedData := base64.StdEncoding.EncodeToString(res)
	respData.EncodedAudio = encodedData

	log.Debugf("Encoded Response Len in hostcall: %d", len(respData.EncodedAudio))

	// return the response
	return respData, nil
}

func ImageGeneration(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	log.Infof("Executing hostcall \"%s\" with args %v", openai.HostCallImageGeneration, args)
	// verify the type of args is ImageGenerationRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	imgGenReq := openai.ImageGenerationRequest{}
	err = imgGenReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("ImageGeneration Request: %s", string(jsonBytes))
	res, err := sendBufferData(bytes.NewBuffer(jsonBytes), "https://api.openai.com/v1/images/generations")
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	respData := openai.ImageGenerationResponse{}
	err = respData.Unmarshal(res)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}
	return respData, nil
}
