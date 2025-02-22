package common

import (
	"os"

	log "github.com/sirupsen/logrus"
)

type OpenAIFunctionType int

type APIEndpointInfo struct {
	Name        string  `json:"name"`
	Model       string  `json:"model"`
	Base        *string `json:"base"`
	APIKey      string  `json:"apikey"`        // used if APIKeyInEnv is empty
	APIKeyInEnv string  `json:"apikey_in_env"` // if not empty, the API key is in env var
	Url         string  `json:"url"`
}

const (
	OpenAIFunctionTypeChatWithTools OpenAIFunctionType = iota
	OpenAIFunctionTypeChatOnly
	OpenAIFunctionTypeEmbeddings
	OpenAIFunctionTypeTextToSpeech
	OpenAIFunctionTypeASR
	OpenAIFunctionTypeImageGeneration
)

var (
	OpenAIBase            = "https://api.openai.com/v1"
	GaiaToolLlamaGroqBase = "https://llamatool.us.gaianet.network/v1"
	GaiaToolLlama70BBase  = "https://llama70b.gaia.domains/v1"
	GaiaToolLlama8BBase   = "https://llama8b.gaia.domains/v1"
	GaiaToolLlama3BBase   = "https://llama3b.gaia.domains/v1"
	GaiaToolQWen72BBase   = "https://qwen72b.gaia.domains/v1"
	GaiaQWen7BBase        = "https://qwen7b.gaia.domains/v1"
	GaiaWhisperBase       = "https://whisper.gaia.domains/v1"
	DeepSeekBase          = "https://api.deepseek.com"
)

var (
	APIEndpointMap = map[OpenAIFunctionType][]APIEndpointInfo{
		OpenAIFunctionTypeChatWithTools: {
			{
				Name:        "deepseek-toolchat",
				Model:       "deepseek-chat",
				Base:        &DeepSeekBase,
				APIKeyInEnv: "DEEPSEEK_API_KEY",
				Url:         "/chat/completions",
			},
			{
				Name:        "openai-toolchat",
				Model:       "gpt-4o",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/chat/completions",
			},
			{
				Name:   "qwen-toolchat-72b",
				Model:  "qwen",
				Base:   &GaiaToolQWen72BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat-70b",
				Model:  "llama",
				Base:   &GaiaToolLlama70BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat",
				Model:  "llama",
				Base:   &GaiaToolLlamaGroqBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat-8b",
				Model:  "llama",
				Base:   &GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat-3b",
				Model:  "llama",
				Base:   &GaiaToolLlama3BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeChatOnly: {
			{
				Name:        "openai-chat",
				Model:       "gpt-4o",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/chat/completions"},
			{
				Name:   "llama-chat",
				Model:  "llama",
				Base:   &GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeEmbeddings: {
			{
				Name:        "openai-embed",
				Model:       "text-embedding-ada-002",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/embeddings",
			},
			{
				Name:   "nomic-embed",
				Model:  "nomic-embed",
				Base:   &GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/embeddings",
			},
		},
		OpenAIFunctionTypeTextToSpeech: {
			{
				Name:        "openai-tts",
				Model:       "tts-1",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/audio/speech",
			},
		},
		OpenAIFunctionTypeImageGeneration: {
			{
				Name:        "openai-genimage",
				Model:       "dall-e-3",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/images/generations",
			},
		},
		OpenAIFunctionTypeASR: {
			{
				Name:   "gaia-whisper",
				Model:  "whisper",
				Base:   &GaiaWhisperBase,
				APIKey: "gaia",
				Url:    "/audio/transcriptions",
			},
			{
				Name:        "openai-whisper",
				Model:       "whisper-1",
				Base:        &OpenAIBase,
				APIKeyInEnv: "OPENAI_API_KEY",
				Url:         "/audio/transcriptions",
			},
		},
	}
)

func GetAPIEndpointInfo(ft OpenAIFunctionType, modelOrName string) []APIEndpointInfo {
	if ft < 0 || int(ft) >= len(APIEndpointMap) {
		return nil
	}
	res := make([]APIEndpointInfo, 0)
	for _, info := range APIEndpointMap[ft] {
		if info.Model == modelOrName || info.Name == modelOrName {
			res = append(res, info)
		}
	}

	// remove if the api key is from env but not set
	res2 := make([]APIEndpointInfo, 0)
	for _, e := range res {
		if e.APIKeyInEnv != "" {
			key := os.Getenv(e.APIKeyInEnv)
			if key == "" {
				// skip if the key is not set
				continue
			}
			res2 = append(res2, e)
			res2[len(res2)-1].APIKey = key
		} else {
			res2 = append(res2, e)
		}
	}

	func() {
		// print the endpoint info found
		tmpList := make([]APIEndpointInfo, 0)
		for _, e := range res2 {
			tmp := &APIEndpointInfo{
				Name:   e.Name,
				Model:  e.Model,
				Base:   e.Base,
				APIKey: "********",
				Url:    e.Url,
			}
			if e.APIKey == "" {
				tmp.APIKey = ""
			}
			tmpList = append(tmpList, *tmp)
		}
		log.Infof("Found %d endpoint(s) for %s: %v", len(tmpList), modelOrName, tmpList)
	}()

	return res2
}

func init() {
	if os.Getenv("OPENAI_API_BASE") != "" {
		// official "https://api.openai.com/v1"
		OpenAIBase = os.Getenv("OPENAI_API_BASE")
	}
}
