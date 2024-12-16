package rpc

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
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

// create request
func NewJsonRPCRequest(method string, params interface{}) *JsonRPCRequest {
	res := &JsonRPCRequest{
		Version: "2.0",
		Method:  &method,
		Params:  params,
	}
	res.ID = nil
	return res
}

func (r *JsonRPCRequest) Send(out io.Writer) error {
	if r.ID == nil {
		return fmt.Errorf("invalid request id")
	}
	b, err := r.Marshal()
	if err != nil {
		return err
	}

	// send little endian length of b using uint64
	length := uint64(len(b))
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, length)
	_, err = out.Write(buf)
	if err != nil {
		return err
	}

	// write b to output pipe
	n, err := out.Write(b)
	if err != nil {
		return err
	}
	if n != len(b) {
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

func NewJsonRPCResponse(id json.Number, result interface{}) *JsonRPCResponse {
	return &JsonRPCResponse{
		Version: "2.0",
		Result:  result,
		ID:      &id,
	}
}

func NewJsonRPCErrorResponse(id json.Number, code int, message string, data interface{}) *JsonRPCResponse {
	return &JsonRPCResponse{
		Version: "2.0",
		Error: &JsonRPCError{
			Code:    code,
			Message: message,
			Data:    data,
		},
		ID: &id,
	}
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

func (r *JsonRPCResponse) Send(out io.Writer) error {
	if r.ID == nil {
		return fmt.Errorf("invalid response id")
	}
	b, err := r.Marshal()
	if err != nil {
		return err
	}

	// send little endian length of b using uint64
	length := uint64(len(b))
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, length)
	_, err = out.Write(buf)
	if err != nil {
		return err
	}

	// write b to output pipe
	n, err := out.Write(b)
	if err != nil {
		return err
	}
	if n != len(b) {
		return fmt.Errorf("error writing to output pipe")
	}
	return nil
}
