package openai

import (
	"encoding/json"
	"fmt"
)

type EmbeddingsRequest struct {
	Input string `json:"input"`
	Model string `json:"model"`
}

func (r *EmbeddingsRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *EmbeddingsRequest) Unmarshal(data []byte) error {
	err := json.Unmarshal(data, r)
	if err != nil {
		return err
	}
	if r.Input == "" || r.Model == "" {
		return fmt.Errorf("invalid input or model")
	}
	return nil
}

type EmbeddingsResponse struct {
	Object string        `json:"object"`
	Data   []interface{} `json:"data"`
	Model  string        `json:"model"`
	Usage  interface{}   `json:"usage"`
}

func (r *EmbeddingsResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *EmbeddingsResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type EmbeddingObject struct {
	Object    string    `json:"object"`
	Embedding []float64 `json:"embedding"`
	Index     int       `json:"index"`
}

func (r *EmbeddingObject) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *EmbeddingObject) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
