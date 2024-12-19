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
	ToolsetID   int            `json:"toolset_id"`
	Params      []NewToolParam `json:"params"`
	Cb          string         `json:"cb"`
}

type NewToolResponse struct {
	ToolsetID int `json:"tool_id"`
}

type NewToolsetRequest struct {
	Name         string `json:"name"`
	Description  string `json:"description"`
	WorkloadName string `json:"workload_name"`
}

type NewToolsetResponse struct {
	ToolsetID int `json:"toolset_id"`
}

type ToolsetInstallBuiltinsRequest struct {
	ToolsetID int `json:"toolset_id"`
}

type ToolsetInstallBuiltinsResponse struct {
	ToolsetID int `json:"toolset_id"`
}

type ToolCallRequest struct {
	ToolsetID int                    `json:"toolset_id"`
	ToolID    int                    `json:"tool_id"`
	Params    map[string]interface{} `json:"params"`
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
