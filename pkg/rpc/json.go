package rpc

import (
	"encoding/json"
	"fmt"
)

type JsonRPCRequest struct {
	Version string      `json:"jsonrpc"`
	Method  string      `json:"method"`
	Params  interface{} `json:"params,omitempty"`
	ID      json.Number `json:"id"`
}

type JsonRPCNotification struct {
	Version string      `json:"jsonrpc"`
	Method  string      `json:"method"`
	Params  interface{} `json:"params"`
}

type JsonRPCError struct {
	Code    int         `json:"code"`
	Message string      `json:"message"`
	Data    interface{} `json:"data"`
}

type JsonRPCResponse struct {
	Version string        `json:"jsonrpc"`
	Result  interface{}   `json:"result,omitempty"`
	Error   *JsonRPCError `json:"error,omitempty"`
	ID      json.Number   `json:"id"`
}

var (
	// global id counter
	idCounter = 0
)

// create request
func NewJsonRPCRequest(method string, params interface{}) *JsonRPCRequest {
	res := &JsonRPCRequest{
		Version: "2.0",
		Method:  method,
		Params:  params,
	}
	idCounter++
	res.ID = json.Number(fmt.Sprint(idCounter))
	return res
}

// Marshal operations

func (r *JsonRPCRequest) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *JsonRPCRequest) CreateSuccessResponse(result interface{}) *JsonRPCResponse {
	return &JsonRPCResponse{
		Version: r.Version,
		Result:  result,
		ID:      r.ID,
	}
}

func (r *JsonRPCRequest) CreateErrorResponse(code int, message string, data interface{}) *JsonRPCResponse {
	return &JsonRPCResponse{
		Version: r.Version,
		Error: &JsonRPCError{
			Code:    code,
			Message: message,
			Data:    data,
		},
		ID: r.ID,
	}
}

func (r *JsonRPCNotification) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

func (r *JsonRPCResponse) Marshal() ([]byte, error) {
	return json.Marshal(r)
}

// Unmarshal operations

func (r *JsonRPCRequest) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *JsonRPCNotification) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *JsonRPCResponse) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}
