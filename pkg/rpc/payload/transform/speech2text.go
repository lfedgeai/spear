package transform

import "encoding/json"

type SpeechToTextRequest struct {
	Model string `json:"model"`
	Audio string `json:"audio"`
}

func (r *SpeechToTextRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *SpeechToTextRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type SpeechToTextResponse struct {
	Text string `json:"text"`
}

func (r *SpeechToTextResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *SpeechToTextResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
