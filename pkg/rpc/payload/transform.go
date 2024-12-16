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
	TransformTypeVector
	TransformTypeUnknown
)

const (
	TransformOperationLLM TransformOperation = iota
	TransformOperationTools
	TransformOperationEmbeddings
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
	Data interface{}   `json:"data"`
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

type TransformConfigRequest struct {
	Test  string `json:"test"`
	Reset bool   `json:"reset"`
}

type TransformConfigResponse struct {
	Result interface{} `json:"result"`
}

// marshal operations
func (r *TransformConfigRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *TransformConfigResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

// unmarshal to TransformRequest
func (r *TransformConfigRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *TransformConfigResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
