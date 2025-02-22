package openai

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"

	"github.com/lfedgeai/spear/pkg/net"
	"github.com/lfedgeai/spear/spearlet/hostcalls/common"
	hcommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
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
	ToolCalls  []OpenAIChatToolCall `json:"tool_calls,omitempty"`
	ToolCallId string               `json:"tool_call_id"`
}

type OpenAIChatCompletionResponse struct {
	Id      string             `json:"id"`
	Model   string             `json:"model"`
	Choices []OpenAIChatChoice `json:"choices"`
}

type OpenAIChatChoice struct {
	Message OpenAIChatMessage `json:"message"`
	Index   int64             `json:"index"`
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
	Messages          []OpenAIChatMessage      `json:"messages"`
	Model             string                   `json:"model"`
	ParallelToolCalls *bool                    `json:"parallel_tool_calls,omitempty"`
	Tools             []OpenAIChatToolFunction `json:"tools,omitempty"`
}

type EndpointInfo struct {
	BaseURL string
	APIKey  string
}

func OpenAIChatCompletion(ep common.APIEndpointInfo, chatReq *OpenAIChatCompletionRequest) (*OpenAIChatCompletionResponse, error) {
	jsonBytes, err := json.Marshal(chatReq)
	if err != nil {
		return nil, fmt.Errorf("error marshalling OpenAIChatCompletionRequest: %v", err)
	}

	// create a https request to https://<base_url>/chat/completions and use b as the request body
	l := *ep.Base
	r := ep.Url
	var u string
	if l[len(l)-1] == '/' && r[0] == '/' {
		u = l[:len(l)-1] + r
	} else {
		u = *ep.Base + ep.Url
	}
	log.Infof("URL: %s", u)
	log.Infof("Request: %s", string(jsonBytes))
	res, err := net.SendRequest(u, bytes.NewBuffer(jsonBytes), net.ContentTypeJSON, ep.APIKey)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	log.Infof("Response: %s", string(res))
	respData := OpenAIChatCompletionResponse{}
	err = json.Unmarshal(res, &respData)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling response: %v. Content: %s",
			err, string(res))
	}

	if respData.Id == "" {
		var tmpMap map[string]map[string]string
		err = json.Unmarshal(res, &tmpMap)
		if err != nil {
			return nil, fmt.Errorf("error unmarshalling response: %v. Content: %s",
				err, res)
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

func Embeddings(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	// // verify the type of args is EmbeddingsRequest
	// // use json marshal and unmarshal to verify the type
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }
	// embeddingsReq := transform.EmbeddingsRequest{}
	// err = embeddingsReq.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// req := OpenAIEmbeddingsRequest{
	// 	Input: embeddingsReq.Input,
	// 	Model: embeddingsReq.Model,
	// }

	// ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeEmbeddings, req.Model)
	// if len(ep) == 0 {
	// 	return nil, fmt.Errorf("error getting endpoint for model %s", req.Model)
	// }
	// resp, err := OpenAIEmbeddings(ep[0], &req)
	// if err != nil {
	// 	return nil, fmt.Errorf("error calling OpenAIEmbeddings: %v", err)
	// }

	// resp2 := transform.EmbeddingsResponse{
	// 	Object: resp.Object,
	// 	Model:  resp.Model,
	// 	Usage:  resp.Usage,
	// }
	// err = utils.InterfaceToType(&resp2.Data, resp.Data)
	// if err != nil {
	// 	return nil, fmt.Errorf("error converting response: %v", err)
	// }

	// return resp2, nil

	return nil, fmt.Errorf("not implemented")
}

func OpenAIEmbeddings(ep common.APIEndpointInfo, args *OpenAIEmbeddingsRequest) (*OpenAIEmbeddingsResponse, error) {
	// verify the type of args is EmbeddingsRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	log.Debugf("Embeddings Request: %s", string(jsonBytes))
	// create a https request to https://api.openai.com/v1/embeddings and use b as the request body
	u := *ep.Base + ep.Url
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

func OpenAITextToSpeech(ep common.APIEndpointInfo, args *OpenAITextToSpeechRequest) (*OpenAITextToSpeechResponse, error) {
	log.Infof("Generating Speech...")
	// verify the type of args is TextToSpeechRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	log.Debugf("TextToSpeech Request: %s", string(jsonBytes))
	u := *ep.Base + ep.Url
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
	Audio []byte `json:"audio"`
}

type OpenAISpeechToTextResponse struct {
	Text string `json:"text"`
}

func OpenAISpeechToText(ep common.APIEndpointInfo, args *OpenAISpeechToTextRequest) (*OpenAISpeechToTextResponse, error) {
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

	log.Debugf("AudioASR Request: %v", sttReq)
	u := *ep.Base + ep.Url

	// send data as multipart/form-data
	payload := &bytes.Buffer{}
	writer := multipart.NewWriter(payload)
	part, err := writer.CreateFormFile("file", "audio.wav")
	if err != nil {
		return nil, fmt.Errorf("error creating form file: %v", err)
	}
	log.Debugf("Audio data: %v", sttReq.Audio)
	// convert base64 encoded audio data to bytes
	data := sttReq.Audio
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

func OpenAIImageGeneration(ep common.APIEndpointInfo, args *OpenAIImageGenerationRequest) (*OpenAIImageGenerationResponse, error) {
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
	u := *ep.Base + ep.Url
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
