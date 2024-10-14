package openai

import "encoding/json"

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
