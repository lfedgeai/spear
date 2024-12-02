package openai

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"

	"github.com/lfedgeai/spear/pkg/net"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

const (
	OpenAIURLBase  = "https://api.openai.com/v1"
	GAIANetURLBase = "https://llamatool.us.gaianet.network/v1"
)

type OpenAIFunctionType int

const (
	OpenAIFunctionTypeChatWithTools OpenAIFunctionType = iota
	OpenAIFunctionTypeChatOnly
	OpenAIFunctionTypeEmbeddings
	OpenAIFunctionTypeTextToSpeech
	OpenAIFunctionTypeSpeechToText
	OpenAIFunctionTypeImageGeneration
)

type APIEndpointInfo struct {
	URL    string
	APIKey string
}

var (
	APIEndpointMap = map[string]map[OpenAIFunctionType]APIEndpointInfo{
		"gpt-4o": {
			OpenAIFunctionTypeChatWithTools: {
				URL:    OpenAIURLBase + "/chat/completions",
				APIKey: os.Getenv("OPENAI_API_KEY"),
			},
			OpenAIFunctionTypeChatOnly: {
				URL:    OpenAIURLBase + "/chat/completions",
				APIKey: os.Getenv("OPENAI_API_KEY"),
			},
		},
		"text-embedding-ada-002": {
			OpenAIFunctionTypeEmbeddings: {
				URL:    OpenAIURLBase + "/embeddings",
				APIKey: os.Getenv("OPENAI_API_KEY"),
			},
		},
		"tts-1": {
			OpenAIFunctionTypeTextToSpeech: {
				URL:    OpenAIURLBase + "/audio/speech",
				APIKey: os.Getenv("OPENAI_API_KEY"),
			},
		},
		"dall-e-3": {
			OpenAIFunctionTypeImageGeneration: {
				URL:    OpenAIURLBase + "/images/generations",
				APIKey: os.Getenv("OPENAI_API_KEY"),
			},
		},
		"llama": {
			OpenAIFunctionTypeChatWithTools: {
				URL:    "https://llamatool.us.gaianet.network/v1/chat/completions",
				APIKey: "gaia",
			},
			OpenAIFunctionTypeChatOnly: {
				URL:    "https://llama8b.gaia.domains/v1/chat/completions",
				APIKey: "gaia",
			},
		},
		"nomic-embed": {
			OpenAIFunctionTypeEmbeddings: {
				URL:    "https://llama8b.gaia.domains/v1/embeddings",
				APIKey: "gaia",
			},
		},
		"whisper": {
			OpenAIFunctionTypeSpeechToText: {
				URL:    "https://whisper.gaia.domains/v1/speech2text",
				APIKey: "gaia",
			},
		},
	}
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

func ModelNameToBaseURLAndAPIKey(modelName string, funcType OpenAIFunctionType) (string, string, error) {
	if APIEndpointMap[modelName] == nil {
		return "", "", fmt.Errorf("model name not found: %s", modelName)
	}
	if APIEndpointMap[modelName][funcType].URL == "" {
		return "", "", fmt.Errorf("function type not found: %v", funcType)
	}
	u := APIEndpointMap[modelName][funcType].URL
	k := APIEndpointMap[modelName][funcType].APIKey
	if k == "" {
		log.Warnf("API Key not found for model: %s and function type: %v", modelName, funcType)
	}
	return u, k, nil
}

func OpenAIChatCompletion(chatReq *OpenAIChatCompletionRequest) (*OpenAIChatCompletionResponse, error) {
	jsonBytes, err := json.Marshal(chatReq)
	if err != nil {
		return nil, fmt.Errorf("error marshalling OpenAIChatCompletionRequest: %v", err)
	}

	// log.Debugf("Chat Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/chat/completions and use b as the request body
	u, k, err := ModelNameToBaseURLAndAPIKey(chatReq.Model, OpenAIFunctionTypeChatWithTools)
	if err != nil {
		return nil, err
	}
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, k)
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
	log.Debugf("Executing hostcall \"%s\" with args %v", openai.HostCallEmbeddings, args)
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

	log.Debugf("Embeddings Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/embeddings and use b as the request body
	u, k, err := ModelNameToBaseURLAndAPIKey(embeddingsReq.Model, OpenAIFunctionTypeEmbeddings)
	if err != nil {
		return nil, err
	}
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, k)
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
	log.Infof("Generating Speech...")
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
	u, k, err := ModelNameToBaseURLAndAPIKey(t2sReq.Model, OpenAIFunctionTypeTextToSpeech)
	if err != nil {
		return nil, err
	}
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, k)
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
	log.Debugf("Executing hostcall \"%s\" with args %v", openai.HostCallImageGeneration, args)
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
	u, k, err := ModelNameToBaseURLAndAPIKey(imgGenReq.Model, OpenAIFunctionTypeImageGeneration)
	if err != nil {
		return nil, err
	}
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, k)
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
