package hostcalls

import (
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	openaihc "github.com/lfedgeai/spear/worker/hostcalls/openai"
)

var Hostcalls = []*hostcalls.HostCall{
	// chat operations
	{
		Name:    openai.HostCallChatCompletion,
		Handler: openaihc.ChatCompletion,
	},
	// text to speech operations
	{
		Name:    openai.HostCallTextToSpeech,
		Handler: openaihc.TextToSpeech,
	},
	// image generation operations
	{
		Name:    openai.HostCallImageGeneration,
		Handler: openaihc.ImageGeneration,
	},
	// embeddings operations
	{
		Name:    openai.HostCallEmbeddings,
		Handler: openaihc.Embeddings,
	},
	// vector store operations
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
	// message passing operations
	{
		Name:    payload.HostCallMessagePassingRegister,
		Handler: MessagePassingRegister,
	},
	{
		Name:    payload.HostCallMessagePassingUnregister,
		Handler: MessagePassingUnregister,
	},
	{
		Name:    payload.HostCallMessagePassingLookup,
		Handler: MessagePassingLookup,
	},
	{
		Name:    payload.HostCallMessagePassingSend,
		Handler: MessagePassingSend,
	},
}