package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/rpc/payload/openai"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	hcopenai "github.com/lfedgeai/spear/worker/hostcalls/openai"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type ChatMessage struct {
	Index    int                    `json:"index"`
	Metadata map[string]interface{} `json:"metadata"`
	Content  string                 `json:"content"`
}

// type ChatMessage struct {
// 	MetaData   map[string]string             `json:"metadata"`
// 	Content    string                        `json:"content"`
// 	ToolCalls  []hcopenai.OpenAIChatToolCall `json:"tool_calls"`
// 	ToolCallId string                        `json:"tool_call_id"`
// }

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

func ChatCompletion(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	log.Debugf("Executing hostcall \"%s\" with args %v", openai.HostCallChatCompletion, args)
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

	msgList, err := OpenAIChatCompletion(inv, &chatReq)
	if err != nil {
		return nil, fmt.Errorf("error calling OpenAIChatCompletion: %v", err)
	}

	var res2 payload.ChatCompletionResponseV2
	// res2.Id = res.Id
	// res2.Model = res.Model
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

func setupOpenAITools(chatReq *hcopenai.OpenAIChatCompletionRequest, task task.Task, toolsetId ToolsetId) error {
	toolset, ok := GetToolset(task, toolsetId)
	if !ok {
		return fmt.Errorf("toolset not found")
	}
	tools := make([]*ToolRegistry, 0)
	for _, toolId := range toolset.toolsIds {
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
				Name:        tool.name,
				Description: tool.description,
				Parameters: hcopenai.OpenAIChatToolParameter{
					Type:                 "object",
					AdditionalProperties: false,
					Properties:           make(map[string]hcopenai.OpenAIChatToolParameterProperty),
				},
			},
		}
		for k, v := range tool.params {
			chatReq.Tools[i].Func.Parameters.Properties[k] = hcopenai.OpenAIChatToolParameterProperty{
				Type:        v.ptype,
				Description: v.description,
			}
			if v.required {
				requiredParams = append(requiredParams, k)
			}
		}
		chatReq.Tools[i].Func.Parameters.Required = requiredParams
		// log.Infof("Tool: %v", chatReq.Tools[i])
	}
	return nil
}

func OpenAIChatCompletion(inv *hostcalls.InvocationInfo, chatReq *payload.ChatCompletionRequest) ([]ChatMessage, error) {
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
			err = setupOpenAITools(&openAiChatReq2, task, ToolsetId(chatReq.ToolsetId))
			if err != nil {
				return nil, fmt.Errorf("error setting up tools: %v", err)
			}
		}

		respData, err = hcopenai.OpenAIChatCompletion(&openAiChatReq2)
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
			} else if choice.Reason == "tool_calls" {
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
					if toolReg, ok := GetToolByName(task, toolCall.Function.Name); ok && toolReg.cb == "" {
						// it is a built-in tool
						fn := toolReg.cbBuiltIn
						if fn == nil {
							return nil, fmt.Errorf("built-in tool not implemented")
						}
						res, err := fn(inv, args)
						if err != nil {
							return nil, fmt.Errorf("error calling built-in tool %s: %v", toolReg.name, err)
						}

						log.Infof("Builtin Tool call response: %v", res)
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
