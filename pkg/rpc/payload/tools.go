package payload

import "encoding/json"

type NewToolParam struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	Description string `json:"description"`
	Required    bool   `json:"required"`
}

type NewToolRequest struct {
	Name        string         `json:"name"`
	Description string         `json:"description"`
	Params      []NewToolParam `json:"params"`
	Cb          string         `json:"cb"`
}

type NewToolResponse struct {
	Tid string `json:"tool_id"`
}

type NewToolsetRequest struct {
	Name        string   `json:"name"`
	Description string   `json:"description"`
	ToolIds     []string `json:"tool_ids"`
}

type NewToolsetResponse struct {
	Tsid string `json:"toolset_id"`
}

type ToolsetInstallBuiltinsRequest struct {
	Tsid string `json:"toolset_id"`
}

type ToolsetInstallBuiltinsResponse struct {
	Tsid string `json:"toolset_id"`
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

func (r *ToolsetInstallBuiltinsRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ToolsetInstallBuiltinsResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *ToolsetInstallBuiltinsRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *ToolsetInstallBuiltinsResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
