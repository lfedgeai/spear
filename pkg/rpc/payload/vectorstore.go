package payload

import "encoding/json"

type VectorStoreCreateRequest struct {
	Name string `json:"name"`
}

func (r *VectorStoreCreateRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreCreateRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type VectorStoreCreateResponse struct {
	VID int `json:"vid"`
}

func (r *VectorStoreCreateResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreCreateResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type VectorStoreDeleteRequest struct {
	VID int `json:"vid"`
}

func (r *VectorStoreDeleteRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreDeleteRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type VectorStoreDeleteResponse struct {
	VID int `json:"vid"`
}

func (r *VectorStoreDeleteResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreDeleteResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type VectorStoreInsertRequest struct {
	VID    int       `json:"vid"`
	Vector []float32 `json:"vector"`
	Data   []byte    `json:"data"`
}

func (r *VectorStoreInsertRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreInsertRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type VectorStoreInsertResponse struct {
	VID int `json:"vid"`
}

func (r *VectorStoreInsertResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *VectorStoreInsertResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
