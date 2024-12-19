package hostcalls

import (
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
)

var Hostcalls = []*hostcalls.HostCall{
	{
		Name:    payload.HostCallTransform,
		Handler: Transform,
	},
	{
		Name:    payload.HostCallTransformConfig,
		Handler: TransformConfig,
	},
	{
		Name:    payload.HostCallToolNew,
		Handler: NewTool,
	},
	{
		Name:    payload.HostCallToolsetNew,
		Handler: NewToolset,
	},
	{
		Name:    payload.HostCallToolsetInstallBuiltins,
		Handler: ToolsetInstallBuiltins,
	},
	{
		Name:    payload.HostCallToolCall,
		Handler: ToolCall,
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
	// input operations
	{
		Name:    payload.HostCallInput,
		Handler: Input,
	},
	// speak operations
	{
		Name:    payload.HostCallSpeak,
		Handler: Speak,
	},
	// record operations
	{
		Name:    payload.HostCallRecord,
		Handler: Record,
	},
}
