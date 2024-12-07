package huggingface

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"

	"github.com/lfedgeai/spear/pkg/net"
	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

type HuggingFaceEmbeddingsRequest struct {
	Inputs string `json:"inputs"`
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

	embeddingsReq2 := HuggingFaceEmbeddingsRequest{
		Inputs: embeddingsReq.Input,
	}

	jsonBytes, err = json.Marshal(embeddingsReq2)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}

	// make sure HUGGINGFACEHUB_API_TOKEN is there
	if os.Getenv("HUGGINGFACEHUB_API_TOKEN") == "" {
		return nil, fmt.Errorf("error getting huggingface api token")
	}
	apiKey := os.Getenv("HUGGINGFACEHUB_API_TOKEN")

	log.Debugf("Embeddings Request: %s", string(jsonBytes))
	res, err := net.SendRequest(
		"https://api-inference.huggingface.co/models/BAAI/bge-large-en-v1.5",
		bytes.NewBuffer(jsonBytes),
		net.ContentTypeJSON,
		apiKey,
	)

	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}

	listRes := []float64{}
	if err := json.Unmarshal(res, &listRes); err != nil {
		log.Errorf("Error unmarshalling data: %v", res)
		return nil, fmt.Errorf("error unmarshalling data. %v", err)
	}
	respData := transform.EmbeddingsResponse{}
	respData.Data = []transform.EmbeddingObject{
		{
			Object:    "embedding",
			Embedding: listRes,
			Index:     0,
		},
	}
	respData.Model = "bge-large-en-v1.5"

	// return the response
	return respData, nil
}
