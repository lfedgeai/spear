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
	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
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
	Name   string
	Model  string
	Base   string
	APIKey string
	Url    string
}

var (
	APIEndpointMap = map[OpenAIFunctionType][]APIEndpointInfo{
		OpenAIFunctionTypeChatWithTools: {
			{
				Name:   "openai chat",
				Model:  "gpt-4o",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/chat/completions",
			},
			{
				Name:   "qwen72b chat",
				Model:  "qwen72b",
				Base:   QWenURLBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama chat",
				Model:  "llama",
				Base:   GAIANetURLBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeChatOnly: {
			{
				Name:   "openai chat no tools",
				Model:  "gpt-4o",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/chat/completions",
			},
			{
				Name:   "llama chat no tools",
				Model:  "llama",
				Base:   "https://llama8b.gaia.domains/v1",
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeEmbeddings: {
			{
				Name:   "openai embeddings",
				Model:  "text-embedding-ada-002",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/embeddings",
			},
			{
				Name:   "nomic embeddings",
				Model:  "nomic-embed",
				Base:   "https://llama8b.gaia.domains/v1",
				APIKey: "gaia",
				Url:    "/embeddings",
			},
		},
		OpenAIFunctionTypeTextToSpeech: {
			{
				Name:   "openai tts",
				Model:  "tts-1",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/audio/speech",
			},
		},
		OpenAIFunctionTypeImageGeneration: {
			{
				Name:   "openai image generation",
				Model:  "dall-e-3",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/images/generations",
			},
		},
		OpenAIFunctionTypeSpeechToText: {
			{
				Name:   "whisper model speech to text",
				Model:  "whisper",
				Base:   "https://whisper.gaia.domains/v1",
				APIKey: "gaia",
				Url:    "/audio/transcriptions",
			},
			{
				Name:   "openai speech to text",
				Model:  "whisper-1",
				Base:   OpenAIURLBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/audio/transcriptions",
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

type EndpointInfo struct {
	BaseURL string
	APIKey  string
}

func EndpointFromTask(t task.Task) EndpointInfo {
	u, ok := t.GetVar(task.TVOpenAIBaseURL)
	if !ok {
		// check environment variable
		env := os.Getenv("OPENAI_API_KEY")
		if env != "" {
			return EndpointInfo{
				BaseURL: OpenAIURLBase,
				APIKey:  env,
			}
		}
		panic("fallback to default base url, env OPENAI_API_KEY not set")
	} else {
		log.Infof("base url found: %s", u)
	}

	k, ok := t.GetVar(task.TVOpenAIAPIKey)
	if !ok {
		log.Errorf("api key not found")
		return EndpointInfo{}
	}

	return EndpointInfo{
		BaseURL: u.(string),
		APIKey:  k.(string),
	}
}

func OpenAIChatCompletion(ep EndpointInfo, chatReq *OpenAIChatCompletionRequest) (*OpenAIChatCompletionResponse, error) {
	jsonBytes, err := json.Marshal(chatReq)
	if err != nil {
		return nil, fmt.Errorf("error marshalling OpenAIChatCompletionRequest: %v", err)
	}

	// log.Debugf("Chat Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/chat/completions and use b as the request body
	u := ep.BaseURL + "/chat/completions"
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, ep.APIKey)
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

type OpenAIEmbeddingsRequest struct {
	Input string `json:"input"`
	Model string `json:"model"`
}

type OpenAIEmbeddingObject struct {
	Object    string    `json:"object"`
	Embedding []float64 `json:"embedding"`
	Index     int       `json:"index"`
}

type OpenAIEmbeddingsResponse struct {
	Object string                  `json:"object"`
	Data   []OpenAIEmbeddingObject `json:"data"`
	Model  string                  `json:"model"`
	Usage  interface{}             `json:"usage"`
}

func Embeddings(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// verify the type of args is EmbeddingsRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	embeddingsReq := transform.EmbeddingsRequest{}
	err = embeddingsReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	req := OpenAIEmbeddingsRequest{
		Input: embeddingsReq.Input,
		Model: embeddingsReq.Model,
	}

	resp, err := OpenAIEmbeddings(EndpointFromTask(*inv.Task), &req)
	if err != nil {
		return nil, fmt.Errorf("error calling OpenAIEmbeddings: %v", err)
	}

	resp2 := transform.EmbeddingsResponse{
		Object: resp.Object,
		Model:  resp.Model,
		Usage:  resp.Usage,
	}
	err = utils.InterfaceToType(&resp2.Data, resp.Data)
	if err != nil {
		return nil, fmt.Errorf("error converting response: %v", err)
	}

	return resp2, nil
}

func OpenAIEmbeddings(ep EndpointInfo, args *OpenAIEmbeddingsRequest) (*OpenAIEmbeddingsResponse, error) {
	// verify the type of args is EmbeddingsRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	log.Debugf("Embeddings Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/embeddings and use b as the request body
	u := ep.BaseURL + "/embeddings"
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, ep.APIKey)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	respData := OpenAIEmbeddingsResponse{}
	err = json.Unmarshal(res, &respData)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}

	// return the response
	return &respData, nil
}

type OpenAITextToSpeechRequest struct {
	Model  string `json:"model"`
	Input  string `json:"input"`
	Voice  string `json:"voice"`
	Format string `json:"response_format"`
}

type OpenAITextToSpeechResponse struct {
	EncodedAudio string `json:"audio"`
}

func OpenAITextToSpeech(ep EndpointInfo, args *OpenAITextToSpeechRequest) (*OpenAITextToSpeechResponse, error) {
	log.Infof("Generating Speech...")
	// verify the type of args is TextToSpeechRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	log.Debugf("TextToSpeech Request: %s", string(jsonBytes))
	u := ep.BaseURL + "/audio/speech"
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, ep.APIKey)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	log.Debugf("OpenAI Response Len: %d", len(res))

	respData := OpenAITextToSpeechResponse{}
	// base64 encode the audio data
	encodedData := base64.StdEncoding.EncodeToString(res)
	respData.EncodedAudio = encodedData

	log.Debugf("Encoded Response Len in hostcall: %d", len(respData.EncodedAudio))

	// return the response
	return &respData, nil
}

type OpenAISpeechToTextRequest struct {
	Model string `json:"model"`
	Audio string `json:"audio"`
}

type OpenAISpeechToTextResponse struct {
	Text string `json:"text"`
}

func OpenAISpeechToText(ep EndpointInfo, args *OpenAISpeechToTextRequest) (*OpenAISpeechToTextResponse, error) {
	log.Infof("Converting Speech to Text...")
	// verify the type of args is SpeechToTextRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	sttReq := OpenAISpeechToTextRequest{}
	err = json.Unmarshal(jsonBytes, &sttReq)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("SpeechToText Request: %v", sttReq)
	u := ep.BaseURL + "/audio/transcriptions"

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
	res, err := net.SendRequest(u, payload, writer.FormDataContentType(), ep.APIKey)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	log.Debugf("Speech to Text Response: %s", string(res))
	respData := OpenAISpeechToTextResponse{}
	err = json.Unmarshal(res, &respData)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}
	return &respData, nil
}

type OpenAIImageGenerationRequest struct {
	Model          string `json:"model"`
	Prompt         string `json:"prompt"`
	ResponseFormat string `json:"response_format"`
}

type OpenAIImageObject struct {
	Url           string `json:"url"`
	B64Json       string `json:"b64_json"`
	RevisedPrompt string `json:"revised_prompt"`
}

type OpenAIImageGenerationResponse struct {
	Created json.Number         `json:"created"`
	Data    []OpenAIImageObject `json:"data"`
}

func OpenAIImageGeneration(ep EndpointInfo, args *OpenAIImageGenerationRequest) (*OpenAIImageGenerationResponse, error) {
	// verify the type of args is ImageGenerationRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	imgGenReq := OpenAIImageGenerationRequest{}
	err = json.Unmarshal(jsonBytes, &imgGenReq)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Debugf("ImageGeneration Request: %s", string(jsonBytes))
	u := ep.BaseURL + "/images/generations"
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, ep.APIKey)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	respData := OpenAIImageGenerationResponse{}
	err = json.Unmarshal(res, &respData)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v", err)
	}
	return &respData, nil
}
