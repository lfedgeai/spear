package common

import (
	"os"
	"strings"

	log "github.com/sirupsen/logrus"
)

type OpenAIFunctionType int

type APIEndpointInfo struct {
	Name   string
	Model  string
	Base   string
	APIKey string
	Url    string
}

const (
	OpenAIFunctionTypeChatWithTools OpenAIFunctionType = iota
	OpenAIFunctionTypeChatOnly
	OpenAIFunctionTypeEmbeddings
	OpenAIFunctionTypeTextToSpeech
	OpenAIFunctionTypeSpeechToText
	OpenAIFunctionTypeImageGeneration
)

const (
	OpenAIBase            = "https://api.openai.com/v1"
	GaiaToolLlamaGroqBase = "https://llamatool.us.gaianet.network/v1"
	GaiaToolLlama8BBase   = "https://llama8b.gaia.domains/v1"
	GaiaToolLlama3BBase   = "https://llama3b.gaia.domains/v1"
	GaiaToolQWen72BBase   = "https://qwen72b.gaia.domains/v1"
	GaiaQWen7BBase        = "https://qwen7b.gaia.domains/v1"
	GaiaWhisperBase       = "https://whisper.gaia.domains/v1"
)

var (
	APIEndpointMap = map[OpenAIFunctionType][]APIEndpointInfo{
		OpenAIFunctionTypeChatWithTools: {
			{
				Name:   "openai-toolchat",
				Model:  "gpt-4o",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/chat/completions",
			},
			{
				Name:   "qwen-toolchat-72b",
				Model:  "qwen",
				Base:   GaiaToolQWen72BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat",
				Model:  "llama",
				Base:   GaiaToolLlamaGroqBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat-8b",
				Model:  "llama",
				Base:   GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
			{
				Name:   "llama-toolchat-3b",
				Model:  "llama",
				Base:   GaiaToolLlama3BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeChatOnly: {
			{
				Name:   "openai-chat",
				Model:  "gpt-4o",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/chat/completions"},
			{
				Name:   "llama-chat",
				Model:  "llama",
				Base:   GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/chat/completions",
			},
		},
		OpenAIFunctionTypeEmbeddings: {
			{
				Name:   "openai-embed",
				Model:  "text-embedding-ada-002",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/embeddings",
			},
			{
				Name:   "nomic-embed",
				Model:  "nomic-embed",
				Base:   GaiaToolLlama8BBase,
				APIKey: "gaia",
				Url:    "/embeddings",
			},
		},
		OpenAIFunctionTypeTextToSpeech: {
			{
				Name:   "openai-tts",
				Model:  "tts-1",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/audio/speech",
			},
		},
		OpenAIFunctionTypeImageGeneration: {
			{
				Name:   "openai-genimage",
				Model:  "dall-e-3",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/images/generations",
			},
		},
		OpenAIFunctionTypeSpeechToText: {
			{
				Name:   "gaia-whisper",
				Model:  "whisper",
				Base:   GaiaWhisperBase,
				APIKey: "gaia",
				Url:    "/audio/transcriptions",
			},
			{
				Name:   "openai-whisper",
				Model:  "whisper-1",
				Base:   OpenAIBase,
				APIKey: os.Getenv("OPENAI_API_KEY"),
				Url:    "/audio/transcriptions",
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
	tmpList := make([]APIEndpointInfo, 0)
	for _, e := range res {
		tmp := &APIEndpointInfo{
			Name:   e.Name,
			Model:  e.Model,
			Base:   e.Base,
			APIKey: "********",
			Url:    e.Url,
		}
		tmpList = append(tmpList, *tmp)
	}
	log.Infof("Found %d endpoints for %s: %v", len(tmpList), modelOrName, tmpList)
	return res
}

func init() {
	if os.Getenv("OPENAI_API_KEY") == "" {
		log.Warnf("OPENAI_API_KEY not set, disabling openai functions")
		newAPIEndpointMap := map[OpenAIFunctionType][]APIEndpointInfo{}
		for ft, infoList := range APIEndpointMap {
			newInfoList := []APIEndpointInfo{}
			for _, info := range infoList {
				// copy data only if the name does not contain "openai"
				if !strings.Contains(info.Name, "openai") {
					newInfoList = append(newInfoList, info)
				}
			}
			newAPIEndpointMap[ft] = newInfoList
		}
		APIEndpointMap = newAPIEndpointMap
	}
}
