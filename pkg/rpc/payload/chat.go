package payload

import (
	"encoding/json"
)

type ChatCompletionRequest struct {
	Messages  []ChatMessageV2 `json:"messages"`
	Model     string          `json:"model"`
	ToolsetId string          `json:"toolset_id"`
}

// marshal operations
func (r *ChatCompletionRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

// unmarshal to ChatCompletionRequest
func (r *ChatCompletionRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type ChatMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type ChatCompletionResponse struct {
	Id      string       `json:"id"`
	Model   string       `json:"model"`
	Choices []ChatChoice `json:"choices"`
}

type ChatChoice struct {
	Message ChatMessage `json:"message"`
	Index   json.Number `json:"index"`
	Reason  string      `json:"finish_reason"`
}

func (r *ChatCompletionResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ChatCompletionResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type ChatMessageV2 struct {
	Metadata map[string]interface{} `json:"metadata"`
	Content  string                 `json:"content"`
}

type ChatCompletionResponseV2 struct {
	Id       string          `json:"id"`
	Model    string          `json:"model"`
	Messages []ChatMessageV2 `json:"messages"`
}

func (r *ChatCompletionResponseV2) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ChatCompletionResponseV2) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type ChatCompletionRequestV2 struct {
	Messages  []ChatMessageV2 `json:"messages"`
	Model     string          `json:"model"`
	ToolsetId string          `json:"toolset_id"`
}

func (r *ChatCompletionRequestV2) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ChatCompletionRequestV2) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
