package payload

import "encoding/json"

type TransformType int
type TransformOperation int

const (
	TransformTypeImage TransformType = iota
	TransformTypeText
	TransformTypeAudio
	TransformTypeVideo
	TransformTypeTensor
	TransformTypeUnknown
)

const (
	TransformOperationLLM TransformOperation = iota
	TransformOperationOCR
	TransformOperationTextToSpeech
	TransformOperationSpeechToText
	TransformOperationTextToImage
)

// Transform request
type TransformRequest struct {
	InputTypes  []TransformType      `json:"input_types"`
	OutputTypes []TransformType      `json:"output_types"`
	Operations  []TransformOperation `json:"operations"`
	Params      interface{}          `json:"params"`
}

type TransformResult struct {
	Type TransformType `json:"type"`
	Data []byte        `json:"data"`
}

// Transform response
type TransformResponse struct {
	Results []TransformResult `json:"results"`
}

// marshal operations
func (r *TransformRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *TransformResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

// unmarshal to TransformRequest
func (r *TransformRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *TransformResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
