package payload

import "encoding/json"

type NewToolParam struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	Description string `json:"description"`
	Required    bool   `json:"required"`
	Cb          string `json:"cb"`
}

type NewToolRequest struct {
	Name        string         `json:"name"`
	Description string         `json:"description"`
	Params      []NewToolParam `json:"params"`
}

type NewToolResponse struct {
	Tid string `json:"tid"`
}

type NewToolsetRequest struct {
	Name        string   `json:"name"`
	Description string   `json:"description"`
	ToolIds     []string `json:"tool_ids"`
}

type NewToolsetResponse struct {
	Tsid string `json:"tsid"`
}

func (r *NewToolRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *NewToolResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *NewToolsetRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *NewToolsetResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *NewToolRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *NewToolResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *NewToolsetRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *NewToolsetResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
