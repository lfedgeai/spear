package hostcalls

import (
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	openaihc "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

var Hostcalls = []*hostcalls.HostCall{
	{
		Name:    openai.HostCallChatCompletion,
		Handler: openaihc.ChatCompletion,
	},
	{
		Name:    openai.HostCallEmbeddings,
		Handler: openaihc.Embeddings,
	},
	{
		Name:    payload.HostCallVectorStoreCreate,
		Handler: VectorStoreCreate,
	},
	{
		Name:    payload.HostCallVectorStoreDelete,
		Handler: VectorStoreDelete,
	},
	{
		Name:    payload.HostCallVectorStoreInsert,
		Handler: VectorStoreInsert,
	},
	{
		Name:    payload.HostCallVectorStoreSearch,
		Handler: VectorStoreSearch,
	},
}
