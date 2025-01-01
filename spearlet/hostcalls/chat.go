package hostcalls

import (
	"encoding/json"
	"fmt"
	"strconv"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/lfedgeai/spear/pkg/spear/proto/chat"
	"github.com/lfedgeai/spear/pkg/spear/proto/tool"
	"github.com/lfedgeai/spear/pkg/spear/proto/transform"
	"github.com/lfedgeai/spear/spearlet/hostcalls/common"
	hcommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	hcopenai "github.com/lfedgeai/spear/spearlet/hostcalls/openai"
	log "github.com/sirupsen/logrus"

	helper "github.com/lfedgeai/spear/pkg/utils/protohelper"
)

type ChatMessage struct {
	Index    int                    `json:"index"`
	Metadata map[string]interface{} `json:"metadata"`
	Content  string                 `json:"content"`
}

type ChatCompletionMemory struct {
	Messages []ChatMessage `json:"messages"`
}

const (
	chatInnerLoopMaxCount = 10
)

func NewChatCompletionMemory() *ChatCompletionMemory {
	return &ChatCompletionMemory{
		Messages: make([]ChatMessage, 0),
	}
}

func (m *ChatCompletionMemory) AddMessage(msg ChatMessage) {
	m.Messages = append(m.Messages, msg)
}

func (m *ChatCompletionMemory) Clear() {
	m.Messages = make([]ChatMessage, 0)
}

func (m *ChatCompletionMemory) GetMessages() []ChatMessage {
	return m.Messages
}

func reasonStrToReason(reason string) chat.Reason {
	switch reason {
	case "stop":
		return chat.ReasonStop
	case "length":
		return chat.ReasonLength
	case "tool_calls":
		return chat.ReasonToolCalls
	default:
		return chat.ReasonOther
	}
}

func reasonToReasonStr(reason chat.Reason) string {
	switch reason {
	case chat.ReasonStop:
		return "stop"
	case chat.ReasonLength:
		return "length"
	case chat.ReasonToolCalls:
		return "tool_calls"
	default:
		return "other"
	}
}

func roleStrToRole(role string) chat.Role {
	switch role {
	case "system":
		return chat.RoleSystem
	case "user":
		return chat.RoleUser
	case "assistant":
		return chat.RoleAssistant
	case "developer":
		return chat.RoleDeveloper
	default:
		return chat.RoleOther
	}
}

func roleToRoleStr(role chat.Role) string {
	switch role {
	case chat.RoleSystem:
		return "system"
	case chat.RoleUser:
		return "user"
	case chat.RoleAssistant:
		return "assistant"
	case chat.RoleDeveloper:
		return "developer"
	default:
		return "other"
	}
}

func chatMessageToTransformBuffer(msgList []ChatMessage) []byte {
	msgOff := make([]flatbuffers.UOffsetT, len(msgList))
	builder := flatbuffers.NewBuilder(1024)
	for i := len(msgList) - 1; i >= 0; i-- {
		content := builder.CreateString(msgList[i].Content)

		chat.ChatMetadataStart(builder)
		chat.ChatMetadataAddRole(builder,
			roleStrToRole(msgList[i].Metadata["role"].(string)))
		if msgList[i].Metadata["reason"] != nil {
			chat.ChatMetadataAddReason(builder,
				reasonStrToReason(msgList[i].Metadata["reason"].(string)))
		}
		metaOff := chat.ChatMetadataEnd(builder)

		chat.ChatMessageStart(builder)
		chat.ChatMessageAddContent(builder, content)
		chat.ChatMessageAddMetadata(builder, metaOff)
		off := chat.ChatMessageEnd(builder)

		msgOff[i] = off
	}

	chat.ChatCompletionResponseStartMessagesVector(builder, len(msgList))
	for i := len(msgList) - 1; i >= 0; i-- {
		builder.PrependUOffsetT(msgOff[i])
	}
	msgs := builder.EndVector(len(msgList))

	chat.ChatCompletionResponseStart(builder)
	chat.ChatCompletionResponseAddMessages(builder, msgs)
	res := chat.ChatCompletionResponseEnd(builder)

	transform.TransformResponseStart(builder)
	transform.TransformResponseAddData(builder, res)
	transform.TransformResponseAddDataType(builder,
		transform.TransformResponse_Dataspear_proto_chat_ChatCompletionResponse)
	builder.Finish(transform.TransformResponseEnd(builder))

	return builder.FinishedBytes()
}

func ChatCompletionWithTools(inv *hcommon.InvocationInfo,
	args *transform.TransformRequest) ([]byte, error) {
	return chatCompletion(inv, args, true)
}

func ChatCompletionNoTools(inv *hcommon.InvocationInfo,
	args *transform.TransformRequest) ([]byte, error) {
	return chatCompletion(inv, args, false)
}

func chatCompletion(inv *hcommon.InvocationInfo, args *transform.TransformRequest,
	hasTool bool) ([]byte, error) {
	// verify the type of args is ChatCompletionRequest
	chatReq := chat.ChatCompletionRequest{}
	if err := helper.UnwrapTransformRequest(&chatReq, args); err != nil {
		return nil, err
	}

	if !hasTool && chatReq.ToolsLength() > 0 {
		log.Infof("Tools are not supported in this function")
		return nil, fmt.Errorf("tools are not supported in this function")
	}

	log.Infof("Using model %s", chatReq.Model())

	msgList, err := innerChatCompletion(inv, &chatReq, hasTool)
	if err != nil {
		return nil, fmt.Errorf("error calling innerChatCompletionNoTools: %v", err)
	}

	buf := chatMessageToTransformBuffer(msgList)
	return buf, nil
}

func innerChatCompletion(inv *hcommon.InvocationInfo, chatReq *chat.ChatCompletionRequest,
	hasTool bool) ([]ChatMessage, error) {
	mem := NewChatCompletionMemory()
	for idx := range chatReq.MessagesLength() {
		msg := chat.ChatMessage{}
		if !chatReq.Messages(&msg, idx) {
			return nil, fmt.Errorf("error getting message")
		}

		meta := chat.ChatMetadata{}
		msg.Metadata(&meta)
		tmp := ChatMessage{
			Metadata: map[string]interface{}{
				"role": roleToRoleStr(meta.Role()),
			},
			Content: string(msg.Content()),
		}
		mem.AddMessage(tmp)
	}

	var respData *hcopenai.OpenAIChatCompletionResponse
	var err error
	var count int
	for count = 0; count < chatInnerLoopMaxCount; count++ {
		// create a new chat request
		openAiChatReq2 := hcopenai.OpenAIChatCompletionRequest{
			Model:    string(chatReq.Model()),
			Messages: []hcopenai.OpenAIChatMessage{},
		}
		// build the messages
		for _, msg := range mem.GetMessages() {
			tmp := hcopenai.OpenAIChatMessage{
				Content: msg.Content,
			}
			if msg.Metadata["role"] != nil {
				tmp.Role = msg.Metadata["role"].(string)
			}
			if msg.Metadata["tool_call_id"] != nil {
				tmp.ToolCallId = msg.Metadata["tool_call_id"].(string)
			}
			if msg.Metadata["tool_calls"] != nil {
				tmp.ToolCalls = msg.Metadata["tool_calls"].([]hcopenai.OpenAIChatToolCall)
			}
			openAiChatReq2.Messages = append(openAiChatReq2.Messages, tmp)
		}
		if hasTool {
			// build the tools
			if chatReq.ToolsLength() > 0 {
				tmp := false
				openAiChatReq2.ParallelToolCalls = &tmp
			}
			for idx := range chatReq.ToolsLength() {
				toolInfo := chat.ToolInfo{}
				if !chatReq.Tools(&toolInfo, idx) {
					return nil, fmt.Errorf("error getting tool info")
				}
				tbl := flatbuffers.Table{}
				if !toolInfo.Data(&tbl) {
					return nil, fmt.Errorf("error getting params")
				}
				switch toolInfo.DataType() {
				case tool.ToolInfoBuiltinToolInfo:
					toolInfo := tool.BuiltinToolInfo{}
					toolInfo.Init(tbl.Bytes, tbl.Pos)
					tool, ok := hcommon.GetBuiltinTool(hcommon.BuiltinToolID(toolInfo.ToolId()))
					if !ok {
						return nil, fmt.Errorf("builtin tool not found")
					}
					requiredParams := make([]string, 0)
					t := hcopenai.OpenAIChatToolFunction{
						Type: "function",
						Func: hcopenai.OpenAIChatToolFunctionSub{
							Name:        fmt.Sprintf("B-%d", tool.Id),
							Description: tool.Description,
							Parameters: hcopenai.OpenAIChatToolParameter{
								Type:                 "object",
								AdditionalProperties: false,
								Properties:           make(map[string]hcopenai.OpenAIChatToolParameterProperty),
							},
						},
					}
					for k, v := range tool.Params {
						t.Func.Parameters.Properties[k] = hcopenai.OpenAIChatToolParameterProperty{
							Type:        v.Ptype,
							Description: v.Description,
						}
						if v.Required {
							requiredParams = append(requiredParams, k)
						}
					}
					t.Func.Parameters.Required = requiredParams
					openAiChatReq2.Tools = append(openAiChatReq2.Tools, t)
				case tool.ToolInfoInternalToolInfo:
					toolInfo := tool.InternalToolInfo{}
					toolInfo.Init(tbl.Bytes, tbl.Pos)
					// TODO: implement this
					panic("not implemented")
				case tool.ToolInfoNormalToolInfo:
					toolInfo := tool.NormalToolInfo{}
					toolInfo.Init(tbl.Bytes, tbl.Pos)
					// TODO: implement this
					panic("not implemented")
				default:
					return nil, fmt.Errorf("unexpected tool info data type")
				}
			}
		}

		ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeChatWithTools,
			openAiChatReq2.Model)
		if len(ep) == 0 {
			return nil, fmt.Errorf("no endpoint found")
		}
		respData, err = hcopenai.OpenAIChatCompletion(ep[0], &openAiChatReq2)
		if err != nil {
			return nil, fmt.Errorf("error calling OpenAIChatCompletion: %v", err)
		}

		if len(respData.Choices) == 0 {
			return nil, fmt.Errorf("no choices found")
		}
		if len(respData.Choices) > 1 {
			return nil, fmt.Errorf("multiple choices found")
		}

		choice := respData.Choices[0]
		if choice.Index != 0 {
			return nil, fmt.Errorf("index mismatch")
		}

		if choice.Reason == "stop" || choice.Reason == "length" {
			mem.AddMessage(ChatMessage{
				Metadata: map[string]interface{}{
					"role":   choice.Message.Role,
					"reason": choice.Reason,
				},
				Content: choice.Message.Content,
			})
			// we have done with the chat
			if count == chatInnerLoopMaxCount {
				count = 0
			}
			break
		} else if choice.Reason == "tool_calls" || len(choice.Message.ToolCalls) > 0 {
			// do tool calls
			if !hasTool {
				log.Errorf("Unexpected tool calls")
				return nil, fmt.Errorf("unexpected tool calls")
			}
			if choice.Message.Content != "" {
				return nil, fmt.Errorf("unexpected content")
			}

			mem.AddMessage(ChatMessage{
				Metadata: map[string]interface{}{
					"role":       choice.Message.Role,
					"tool_calls": choice.Message.ToolCalls,
				},
				Content: choice.Message.Content,
			})

			toolCalls := choice.Message.ToolCalls
			for _, toolCall := range toolCalls {
				argsStr := toolCall.Function.Arguments
				// use json to unmarshal the arguments to interface{}
				var args interface{} = nil
				if argsStr != "" {
					err := json.Unmarshal([]byte(argsStr), &args)
					if err != nil {
						return nil, fmt.Errorf("error unmarshalling tool call arguments: %v",
							err)
					}
				}
				// check the tool type here
				// the tool name should be in the format of "B-<tool id>" when it is a built-in tool
				// the tool name should be in the format of "I-<tool name>" when it is an internal tool
				// the tool name should be in the format of "N-<tool name>" when it is a normal tool
				toolName := toolCall.Function.Name
				if len(toolName) < 3 || toolName[1] != '-' {
					return nil, fmt.Errorf("invalid tool name")
				}
				toolType := toolName[:1]
				// convert string to uint16 using strconv
				toolId, err := strconv.ParseUint(toolName[2:], 10, 16)
				if err != nil {
					return nil, fmt.Errorf("error parsing tool id: %v", err)
				}
				switch toolType {
				case "B":
					// it is a built-in tool
					toolReg, ok := hcommon.GetBuiltinTool(hcommon.BuiltinToolID(toolId))
					if !ok {
						return nil, fmt.Errorf("builtin tool not found")
					}
					fn := toolReg.CbBuiltIn
					if fn == nil {
						return nil, fmt.Errorf("built-in tool not implemented")
					}
					res, err := fn(inv, args)
					if err != nil {
						return nil, fmt.Errorf("error calling built-in tool %s: %v",
							toolReg.Name, err)
					}

					tmp := fmt.Sprintf("%v", res)
					if len(tmp) > 512 {
						tmp = tmp[:509] + "..."
					}
					log.Infof("Builtin Tool call response: %v", tmp)

					mem.AddMessage(ChatMessage{
						Metadata: map[string]interface{}{
							"role":         "tool",
							"tool_call_id": toolCall.Id,
						},
						Content: fmt.Sprintf("%v", res),
					})
				case "I":
				// it is an internal tool
				case "N":
				// it is a normal tool
				default:
					return nil, fmt.Errorf("invalid tool type")
				}
			}
		} else {
			return nil, fmt.Errorf("unexpected reason: %s", choice.Reason)
		}
	}

	if count == chatInnerLoopMaxCount {
		return nil, fmt.Errorf("max count reached")
	}

	return mem.GetMessages(), nil
}
