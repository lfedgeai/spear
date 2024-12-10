package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/worker/hostcalls/common"
	hcommon "github.com/lfedgeai/spear/worker/hostcalls/common"
	hcopenai "github.com/lfedgeai/spear/worker/hostcalls/openai"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type ChatMessage struct {
	Index    int                    `json:"index"`
	Metadata map[string]interface{} `json:"metadata"`
	Content  string                 `json:"content"`
}

type ChatCompletionMemory struct {
	Messages []ChatMessage `json:"messages"`
}

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

func ChatCompletionNoTools(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	// verify the type of args is ChatCompletionRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	chatReq := payload.ChatCompletionRequest{}
	err = chatReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	if chatReq.ToolsetId != "" {
		log.Infof("Tools are not supported in this function")
		return nil, fmt.Errorf("tools are not supported in this function")
	}

	log.Infof("Using model %s", chatReq.Model)

	msgList, err := innerChatCompletionNoTools(inv, &chatReq)
	if err != nil {
		return nil, fmt.Errorf("error calling innerChatCompletionNoTools: %v", err)
	}

	var res2 payload.ChatCompletionResponseV2
	res2.Messages = make([]payload.ChatMessageV2, len(msgList))
	for i, msg := range msgList {
		md := map[string]interface{}{
			"role": msg.Metadata["role"],
		}
		if msg.Metadata["reason"] != nil {
			md["reason"] = msg.Metadata["reason"]
		}
		res2.Messages[i] = payload.ChatMessageV2{
			Metadata: md,
			Content:  msg.Content,
		}
	}
	return res2, nil
}

func ChatCompletionWithTools(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	// verify the type of args is ChatCompletionRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	chatReq := payload.ChatCompletionRequest{}
	err = chatReq.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Infof("Using model %s", chatReq.Model)

	msgList, err := innerChatCompletionWithTools(inv, &chatReq)
	if err != nil {
		return nil, fmt.Errorf("error calling innerChatCompletionWithTools: %v", err)
	}

	var res2 payload.ChatCompletionResponseV2
	res2.Messages = make([]payload.ChatMessageV2, len(msgList))
	for i, msg := range msgList {
		md := map[string]interface{}{
			"role": msg.Metadata["role"],
		}
		if msg.Metadata["reason"] != nil {
			md["reason"] = msg.Metadata["reason"]
		}
		if msg.Metadata["tool_call_id"] != nil {
			md["tool_call_id"] = msg.Metadata["tool_call_id"]
		}
		if msg.Metadata["tool_calls"] != nil {
			md["tool_calls"] = msg.Metadata["tool_calls"]
		}
		res2.Messages[i] = payload.ChatMessageV2{
			Metadata: md,
			Content:  msg.Content,
		}
	}
	return res2, nil
}

func setupOpenAITools(chatReq *hcopenai.OpenAIChatCompletionRequest, task task.Task, toolsetId hcommon.ToolsetId) error {
	toolset, ok := GetToolset(task, toolsetId)
	if !ok {
		return fmt.Errorf("toolset not found")
	}
	tools := make([]*hcommon.ToolRegistry, 0)
	for _, toolId := range toolset.ToolsIds {
		tool, ok := GetToolById(task, toolId)
		if ok {
			tools = append(tools, tool)
		}
	}
	if len(tools) == 0 {
		return fmt.Errorf("no tools found in toolset")
	}
	chatReq.Tools = make([]hcopenai.OpenAIChatToolFunction, len(tools))
	for i, tool := range tools {
		requiredParams := make([]string, 0)
		chatReq.Tools[i] = hcopenai.OpenAIChatToolFunction{
			Type: "function",
			Func: hcopenai.OpenAIChatToolFunctionSub{
				Name:        tool.Name,
				Description: tool.Description,
				Parameters: hcopenai.OpenAIChatToolParameter{
					Type:                 "object",
					AdditionalProperties: false,
					Properties:           make(map[string]hcopenai.OpenAIChatToolParameterProperty),
				},
			},
		}
		for k, v := range tool.Params {
			chatReq.Tools[i].Func.Parameters.Properties[k] = hcopenai.OpenAIChatToolParameterProperty{
				Type:        v.Ptype,
				Description: v.Description,
			}
			if v.Required {
				requiredParams = append(requiredParams, k)
			}
		}
		chatReq.Tools[i].Func.Parameters.Required = requiredParams
		// log.Infof("Tool: %v", chatReq.Tools[i])
	}
	return nil
}

func innerChatCompletionWithTools(inv *hcommon.InvocationInfo, chatReq *payload.ChatCompletionRequest) ([]ChatMessage, error) {
	task := *(inv.Task)

	mem := NewChatCompletionMemory()
	for _, msg := range chatReq.Messages {
		tmp := ChatMessage{
			Metadata: msg.Metadata,
			Content:  msg.Content,
		}
		mem.AddMessage(tmp)
	}

	finished := false
	var respData *hcopenai.OpenAIChatCompletionResponse
	var err error
	for !finished {
		// create a new chat request
		openAiChatReq2 := hcopenai.OpenAIChatCompletionRequest{
			Model:    chatReq.Model,
			Messages: []hcopenai.OpenAIChatMessage{},
		}
		for _, msg := range mem.GetMessages() {
			tmp := hcopenai.OpenAIChatMessage{
				Content: msg.Content,
			}
			if msg.Metadata["role"] != nil {
				tmp.Role = msg.Metadata["role"].(string)
			}
			if msg.Metadata["tool_calls"] != nil {
				log.Debugf("Tool calls: %v", msg.Metadata["tool_calls"])
				switch msg.Metadata["tool_calls"].(type) {
				case []hcopenai.OpenAIChatToolCall:
					tmp.ToolCalls = msg.Metadata["tool_calls"].([]hcopenai.OpenAIChatToolCall)
				case []interface{}:
					// marshal the interface{} to json and unmarshal to OpenAIChatToolCall
					toolCalls := msg.Metadata["tool_calls"].([]interface{})
					toolCallsStr, err := json.Marshal(toolCalls)
					if err != nil {
						return nil, fmt.Errorf("error marshalling tool calls: %v", err)
					}
					var toolCalls2 []hcopenai.OpenAIChatToolCall
					err = json.Unmarshal(toolCallsStr, &toolCalls2)
					if err != nil {
						return nil, fmt.Errorf("error unmarshalling tool calls: %v", err)
					}
					tmp.ToolCalls = toolCalls2
				default:
					return nil, fmt.Errorf("unexpected type for tool_calls")
				}
			}
			if msg.Metadata["tool_call_id"] != nil {
				tmp.ToolCallId = msg.Metadata["tool_call_id"].(string)
			}
			openAiChatReq2.Messages = append(openAiChatReq2.Messages, tmp)
		}

		// check if toolset exists
		if chatReq.ToolsetId != "" {
			err = setupOpenAITools(&openAiChatReq2, task, hcommon.ToolsetId(chatReq.ToolsetId))
			if err != nil {
				return nil, fmt.Errorf("error setting up tools: %v", err)
			}
		}

		ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeChatWithTools, openAiChatReq2.Model)
		if len(ep) == 0 {
			return nil, fmt.Errorf("no endpoint found")
		}
		respData, err = hcopenai.OpenAIChatCompletion(ep[0], &openAiChatReq2)
		if err != nil {
			return nil, fmt.Errorf("error calling OpenAIChatCompletion: %v", err)
		}

		log.Debugf("Response: %v", respData)

		for i, choice := range respData.Choices {
			if choice.Index != json.Number(fmt.Sprintf("%d", i)) {
				return nil, fmt.Errorf("index mismatch")
			}
			log.Infof("Reason: %s", choice.Reason)
			if (choice.Reason == "stop" && len(choice.Message.ToolCalls) == 0) || choice.Reason == "length" {
				mem.AddMessage(ChatMessage{
					Metadata: map[string]interface{}{
						"role":   choice.Message.Role,
						"reason": choice.Reason,
					},
					Content: choice.Message.Content,
				})
				finished = true
			} else if choice.Reason == "tool_calls" || len(choice.Message.ToolCalls) > 0 {
				mem.AddMessage(ChatMessage{
					Metadata: map[string]interface{}{
						"role":       choice.Message.Role,
						"tool_calls": choice.Message.ToolCalls,
						"reason":     choice.Reason,
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
							return nil, fmt.Errorf("error unmarshalling tool call arguments: %v", err)
						}
					}
					if toolReg, ok := GetToolByName(task, toolCall.Function.Name); ok && toolReg.Cb == "" {
						// it is a built-in tool
						fn := toolReg.CbBuiltIn
						if fn == nil {
							return nil, fmt.Errorf("built-in tool not implemented")
						}
						res, err := fn(inv, args)
						if err != nil {
							return nil, fmt.Errorf("error calling built-in tool %s: %v", toolReg.Name, err)
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
					} else {
						err = inv.CommMgr.SendOutgoingRPCRequestCallback(task, toolCall.Function.Name, args, func(resp *rpc.JsonRPCResponse) error {
							log.Infof("External Tool call response: %v", resp)
							return nil
						})
						if err != nil {
							return nil, fmt.Errorf("error sending tool call: %v", err)
						}
					}
				}
			} else {
				return nil, fmt.Errorf("unexpected reason: %s", choice.Reason)
			}
		}
	}

	return mem.GetMessages(), nil
}

func innerChatCompletionNoTools(inv *hcommon.InvocationInfo, chatReq *payload.ChatCompletionRequest) ([]ChatMessage, error) {
	mem := NewChatCompletionMemory()
	for _, msg := range chatReq.Messages {
		tmp := ChatMessage{
			Metadata: msg.Metadata,
			Content:  msg.Content,
		}
		mem.AddMessage(tmp)
	}

	finished := false
	var respData *hcopenai.OpenAIChatCompletionResponse
	var err error
	for !finished {
		// create a new chat request
		openAiChatReq2 := hcopenai.OpenAIChatCompletionRequest{
			Model:    chatReq.Model,
			Messages: []hcopenai.OpenAIChatMessage{},
		}
		for _, msg := range mem.GetMessages() {
			tmp := hcopenai.OpenAIChatMessage{
				Content: msg.Content,
			}
			if msg.Metadata["role"] != nil {
				tmp.Role = msg.Metadata["role"].(string)
			}
			openAiChatReq2.Messages = append(openAiChatReq2.Messages, tmp)
		}

		ep := common.GetAPIEndpointInfo(common.OpenAIFunctionTypeChatWithTools, openAiChatReq2.Model)
		if len(ep) == 0 {
			return nil, fmt.Errorf("no endpoint found")
		}
		respData, err = hcopenai.OpenAIChatCompletion(ep[0], &openAiChatReq2)
		if err != nil {
			return nil, fmt.Errorf("error calling OpenAIChatCompletion: %v", err)
		}

		log.Debugf("Response: %v", respData)

		for i, choice := range respData.Choices {
			if choice.Index != json.Number(fmt.Sprintf("%d", i)) {
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
				finished = true
			} else {
				return nil, fmt.Errorf("unexpected reason: %s", choice.Reason)
			}
		}
	}

	return mem.GetMessages(), nil
}
