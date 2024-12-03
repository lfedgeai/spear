package openai

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"os"

	"github.com/lfedgeai/spear/pkg/net"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

const (
	OpenAIURLBase  = "https://api.openai.com/v1"
	GAIANetURLBase = "https://llamatool.us.gaianet.network/v1"
	QWenURLBase    = "https://qwen72b.gaia.domains/v1"
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
	APIEndpointMap = map[string]map[OpenAIFunctionType][]APIEndpointInfo{
		"gpt-4o": {
			OpenAIFunctionTypeChatWithTools: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/chat/completions",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
			},
			OpenAIFunctionTypeChatOnly: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/chat/completions",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
			},
		},
		"qwen72b": {
			OpenAIFunctionTypeChatWithTools: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/chat/completions",
					APIKey: "gaia",
				},
			},
		},
		"text-embedding-ada-002": {
			OpenAIFunctionTypeEmbeddings: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/embeddings",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
			},
		},
		"tts-1": {
			OpenAIFunctionTypeTextToSpeech: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/audio/speech",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
			},
		},
		"dall-e-3": {
			OpenAIFunctionTypeImageGeneration: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/images/generations",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
			},
		},
		"llama": {
			OpenAIFunctionTypeChatWithTools: []APIEndpointInfo{
				{
					URL:    "https://llamatool.us.gaianet.network/v1/chat/completions",
					APIKey: "gaia",
				},
			},
			OpenAIFunctionTypeChatOnly: []APIEndpointInfo{
				{
					URL:    "https://llama8b.gaia.domains/v1/chat/completions",
					APIKey: "gaia",
				},
			},
		},
		"nomic-embed": {
			OpenAIFunctionTypeEmbeddings: []APIEndpointInfo{
				{
					URL:    "https://llama8b.gaia.domains/v1/embeddings",
					APIKey: "gaia",
				},
			},
		},
		"whisper": {
			OpenAIFunctionTypeSpeechToText: []APIEndpointInfo{
				{
					URL:    "https://whisper.gaia.domains/v1/audio/transcriptions",
					APIKey: "gaia",
				},
			},
		},
		"whisper-1": {
			OpenAIFunctionTypeSpeechToText: []APIEndpointInfo{
				{
					URL:    OpenAIURLBase + "/audio/transcriptions",
					APIKey: os.Getenv("OPENAI_API_KEY"),
				},
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
	if APIEndpointMap[modelName][funcType] == nil {
		return "", "", fmt.Errorf("function type not found: %v", funcType)
	}
	var e APIEndpointInfo
	for _, info := range APIEndpointMap[modelName][funcType] {
		if info.APIKey != "" {
			e = info
		}
	}

	if e.URL == "" {
		return "", "", fmt.Errorf("function type not found: %v", funcType)
	}
	u := e.URL
	k := e.APIKey
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

func SpeechToText(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	log.Infof("Converting Speech to Text...")
	// verify the type of args is SpeechToTextRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	sttReq := openai.SpeechToTextRequest{}
	err = sttReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("SpeechToText Request: %v", sttReq)
	u, k, err := ModelNameToBaseURLAndAPIKey(sttReq.Model, OpenAIFunctionTypeSpeechToText)
	if err != nil {
		return nil, err
	}

	// send data as multipart/form-data
	payload := &bytes.Buffer{}
	writer := multipart.NewWriter(payload)
	part, err := writer.CreateFormFile("file", "audio.wav")
	if err != nil {
		return nil, fmt.Errorf("error creating form file: %v", err)
	}
	log.Debugf("Audio data: %v", sttReq.Audio)
	// convert base64 encoded audio data to bytes
	data := make([]byte, base64.StdEncoding.DecodedLen(len(sttReq.Audio)))
	n, err := base64.StdEncoding.Decode(data, []byte(sttReq.Audio))
	if err != nil {
		return nil, fmt.Errorf("error decoding audio data: %v", err)
	}
	log.Debugf("Decoded audio data len: %d", n)
	_, err = io.Copy(part, bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("error copying audio data: %v", err)
	}
	part2, err := writer.CreateFormField("model")
	if err != nil {
		return nil, fmt.Errorf("error creating form field: %v", err)
	}
	_, err = part2.Write([]byte(sttReq.Model))
	if err != nil {
		return nil, fmt.Errorf("error writing model data: %v", err)
	}
	err = writer.Close()
	if err != nil {
		return nil, fmt.Errorf("error closing writer: %v", err)
	}

	// send the request
	res, err := net.SendRequest(u, payload, writer.FormDataContentType(), k)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	log.Debugf("Speech to Text Response: %s", string(res))
	respData := openai.SpeechToTextResponse{}
	err = respData.Unmarshal(res)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}
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
