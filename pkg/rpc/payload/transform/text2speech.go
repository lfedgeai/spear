package transform

import "encoding/json"

type TextToSpeechRequest struct {
	Model  string `json:"model"`
	Input  string `json:"input"`
	Voice  string `json:"voice"`
	Format string `json:"response_format"`
}

func (r *TextToSpeechRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *TextToSpeechRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type TextToSpeechResponse struct {
	EncodedAudio string `json:"audio"`
}

func (r *TextToSpeechResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *TextToSpeechResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
