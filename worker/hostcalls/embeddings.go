package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/hostcalls/huggingface"
	openaihc "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

type EmbeddingFunc func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error)

var (
	globalEmbeddings = map[string]EmbeddingFunc{
		"text-embedding-ada-002": openaihc.Embeddings,
		"bge-large-en-v1.5":      huggingface.Embeddings,
	}
)

func Embeddings(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	embeddingsReq := openai.EmbeddingsRequest{}
	err = embeddingsReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	for k, v := range globalEmbeddings {
		if k == embeddingsReq.Model {
			return v(inv, args)
		}
	}
	return nil, fmt.Errorf("embedding not found")
}
