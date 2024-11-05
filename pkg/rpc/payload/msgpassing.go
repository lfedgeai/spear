package payload

import "encoding/json"

type MessagePassingRegisterRequest struct {
	Name   string `json:"name"`
	Method string `json:"method"`
}

func (r *MessagePassingRegisterRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingRegisterRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingRegisterResponse struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
}

func (r *MessagePassingRegisterResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingRegisterResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingUnregisterRequest struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
}

func (r *MessagePassingUnregisterRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingUnregisterRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingUnregisterResponse struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
}

func (r *MessagePassingUnregisterResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingUnregisterResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingLookupRequest struct {
	Name string `json:"name"`
}

func (r *MessagePassingLookupRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingLookupRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingLookupResponse struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
}

func (r *MessagePassingLookupResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingLookupResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingSendRequest struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
	Data         []byte `json:"data"`
}

func (r *MessagePassingSendRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingSendRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

type MessagePassingSendResponse struct {
	MsgPassingId uint64 `json:"msg_passing_id"`
}

func (r *MessagePassingSendResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *MessagePassingSendResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
