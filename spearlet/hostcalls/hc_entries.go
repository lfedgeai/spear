package hostcalls

import (
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
)

var Hostcalls = []*hostcalls.HostCall{
	{
		NameID:  transport.MethodTransform,
		Handler: Transform,
	},
	{
		NameID:  transport.MethodTransformConfig,
		Handler: TransformConfig,
	},
	// invoke tool
	{
		NameID:  transport.MethodToolInvoke,
		Handler: nil,
	},
	{
		NameID:  transport.MethodInternalToolCreate,
		Handler: NewInternalTool,
	},
	// // chat operations
	// {
	// 	NameID:    transform.HostCallChatCompletion,
	// 	Handler: ChatCompletionWithTools,
	// },
	// // text to speech operations
	// {
	// 	NameID:    openai.HostCallTextToSpeech,
	// 	Handler: openaihc.TextToSpeech,
	// },
	// // image generation operations
	// {
	// 	NameID:    openai.HostCallImageGeneration,
	// 	Handler: openaihc.ImageGeneration,
	// },
	// // embeddings operations
	// {
	// 	NameID:    openai.HostCallEmbeddings,
	// 	Handler: openaihc.Embeddings,
	// },
	// vector store operations
	{
		NameID:  transport.MethodVecStoreCreate,
		Handler: VectorStoreCreate,
	},
	{
		NameID:  transport.MethodVecStoreDelete,
		Handler: VectorStoreDelete,
	},
	{
		NameID:  transport.MethodVecStoreInsert,
		Handler: VectorStoreInsert,
	},
	{
		NameID:  transport.MethodVecStoreQuery,
		Handler: VectorStoreSearch,
	},
	// message passing operations
	// {
	// 	NameID:  payload.HostCallMessagePassingRegister,
	// 	Handler: MessagePassingRegister,
	// },
	// {
	// 	NameID:  payload.HostCallMessagePassingUnregister,
	// 	Handler: MessagePassingUnregister,
	// },
	// {
	// 	NameID:  payload.HostCallMessagePassingLookup,
	// 	Handler: MessagePassingLookup,
	// },
	// {
	// 	NameID:  payload.HostCallMessagePassingSend,
	// 	Handler: MessagePassingSend,
	// },
	// input operations
	{
		NameID:  transport.MethodInput,
		Handler: Input,
	},
	// speak operations
	{
		NameID:  transport.MethodSpeak,
		Handler: Speak,
	},
	// record operations
	{
		NameID:  transport.MethodRecord,
		Handler: Record,
	},
	// custom operations
	{
		NameID:  transport.MethodCustom,
		Handler: nil,
	},
}
