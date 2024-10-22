package openai

import (
	"encoding/json"
)

type ChatCompletionRequest struct {
	Messages []ChatMessage `json:"messages"`
	Model    string        `json:"model"`
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
