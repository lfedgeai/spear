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

var (
	gptModels = map[string]struct{}{
		"gpt-4o": {},
	}
)

type ChatMessage struct {
	MetaData   map[string]string             `json:"meta_data"`
	Content    string                        `json:"content"`
	ToolCalls  []hcopenai.OpenAIChatToolCall `json:"tool_calls"`
	ToolCallId string                        `json:"tool_call_id"`
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

func ChatCompletion(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	log.Infof("Executing hostcall \"%s\" with args %v", openai.HostCallChatCompletion, args)
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

	if _, ok := gptModels[chatReq.Model]; ok {
		log.Infof("Using GPT-4o model")
		// process gpt-4o model chat completion
		// convert chatReq to OpenAIChatCompletionRequest

		return OpenAIChatCompletion(inv, &chatReq)
	}

	// TODO: Implement the actual call

	return nil, fmt.Errorf("not implemented")
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

func OpenAIChatCompletion(inv *hostcalls.InvocationInfo, chatReq *payload.ChatCompletionRequest) (*payload.ChatCompletionResponse, error) {
	task := *(inv.Task)

	mem := NewChatCompletionMemory()
	for _, msg := range chatReq.Messages {
		mem.AddMessage(ChatMessage{
			MetaData: map[string]string{
				"role": msg.Role,
			},
			Content: msg.Content,
		})
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
			openAiChatReq2.Messages = append(openAiChatReq2.Messages,
				hcopenai.OpenAIChatMessage{
					Role:       msg.MetaData["role"],
					Content:    msg.Content,
					ToolCalls:  msg.ToolCalls,
					ToolCallId: msg.ToolCallId,
				})
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

		log.Infof("Response: %v", respData)

		for i, choice := range respData.Choices {
			if choice.Index != json.Number(fmt.Sprintf("%d", i)) {
				return nil, fmt.Errorf("index mismatch")
			}
			if choice.Reason == "stop" || choice.Reason == "length" {
				mem.AddMessage(ChatMessage{
					MetaData: map[string]string{
						"role": choice.Message.Role,
					},
					Content: choice.Message.Content,
				})
				finished = true
			} else if choice.Reason == "tool_calls" {
				mem.AddMessage(ChatMessage{
					MetaData: map[string]string{
						"role": choice.Message.Role,
					},
					Content:   choice.Message.Content,
					ToolCalls: choice.Message.ToolCalls,
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
							return nil, fmt.Errorf("error calling built-in tool: %v", err)
						}

						log.Infof("Builtin Tool call response: %v", res)
						mem.AddMessage(ChatMessage{
							MetaData: map[string]string{
								"role": "tool",
							},
							Content:    fmt.Sprintf("%v", res),
							ToolCallId: toolCall.Id,
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

	resp := &payload.ChatCompletionResponse{}
	resp.Choices = []payload.ChatChoice{}
	for i, msg := range mem.GetMessages() {
		resp.Choices = append(resp.Choices, payload.ChatChoice{
			Index: json.Number(fmt.Sprintf("%d", i)),
			Message: payload.ChatMessage{
				Role:    msg.MetaData["role"],
				Content: msg.Content,
			},
		})
	}

	return resp, nil
}
