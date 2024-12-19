package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/utils"
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
	embeddingsReq := transform.EmbeddingsRequest{}
	if err := utils.InterfaceToType(&embeddingsReq, args); err != nil {
		return nil, err
	}

	for k, v := range globalEmbeddings {
		if k == embeddingsReq.Model {
			return v(inv, args)
		}
	}
	return nil, fmt.Errorf("embedding not found")
}
