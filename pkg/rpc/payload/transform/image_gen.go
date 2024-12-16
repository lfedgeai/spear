package transform

import "encoding/json"

type ImageGenerationRequest struct {
	Model          string `json:"model"`
	Prompt         string `json:"prompt"`
	ResponseFormat string `json:"response_format"`
}

func (r *ImageGenerationRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ImageGenerationRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type ImageObject struct {
	Url           string `json:"url"`
	B64Json       string `json:"b64_json"`
	RevisedPrompt string `json:"revised_prompt"`
}

type ImageGenerationResponse struct {
	Created json.Number   `json:"created"`
	Data    []ImageObject `json:"data"`
}

func (r *ImageGenerationResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ImageGenerationResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
