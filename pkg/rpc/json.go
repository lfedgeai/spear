package rpc

import (
	"encoding/json"
	"fmt"
	"os"
)

type JsonRPCRequest struct {
	Version string       `json:"jsonrpc"`
	Method  *string      `json:"method"`
	Params  interface{}  `json:"params,omitempty"`
	ID      *json.Number `json:"id"`
}

type JsonRPCNotification struct {
	Version string      `json:"jsonrpc"`
	Method  *string     `json:"method"`
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
	ID      *json.Number  `json:"id"`
}

var (
	// global id counter
	idCounter = 0
)

// create request
func NewJsonRPCRequest(method string, params interface{}) *JsonRPCRequest {
	res := &JsonRPCRequest{
		Version: "2.0",
		Method:  &method,
		Params:  params,
	}
	idCounter++
	tmp := json.Number(fmt.Sprintf("%d", idCounter))
	res.ID = &tmp
	return res
}

func (r *JsonRPCRequest) Send(out *os.File) error {
	b, err := r.Marshal()
	if err != nil {
		return err
	}
	// write b + '\n' to output pipe
	n, err := out.Write(append(b, '\n'))
	if err != nil {
		return err
	}
	if n != len(b)+1 {
		return fmt.Errorf("error writing to output pipe")
	}
	return nil
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
	err := json.Unmarshal(data, r)
	if err != nil {
		return err
	}
	if r.Method == nil || r.ID == nil {
		return fmt.Errorf("invalid request")
	}
	return nil
}

func (r *JsonRPCNotification) Unmarshal(data []byte) error {
	return json.Unmarshal(data, r)
}

func (r *JsonRPCResponse) Unmarshal(data []byte) error {
	err := json.Unmarshal(data, r)
	if err != nil {
		return err
	}
	if r.ID == nil {
		return fmt.Errorf("invalid response")
	}
	if r.Error != nil && r.Result != nil {
		return fmt.Errorf("invalid response")
	}
	return nil
}
