package protohelper

import (
	"fmt"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/lfedgeai/spear/pkg/spear/proto/transform"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
)

type IfWithInit[T any] interface {
	*T
	Init([]byte, flatbuffers.UOffsetT)
}

func UnwrapTransformRequest[T any, P IfWithInit[T]](d P, s *transform.TransformRequest) error {
	if d == nil {
		return fmt.Errorf("destination is nil")
	}
	if s == nil {
		return fmt.Errorf("source is nil")
	}
	tbl := flatbuffers.Table{}
	if !s.Params(&tbl) {
		return fmt.Errorf("error getting params")
	}
	d.Init(tbl.Bytes, tbl.Pos)
	return nil
}

func CreateErrorTransportResponse(id int64, code int,
	msg string) *transport.TransportResponse {
	builder := flatbuffers.NewBuilder(0)
	msgOff := builder.CreateString(msg)

	transport.TransportResponseStart(builder)
	transport.TransportResponseAddId(builder, id)
	transport.TransportResponseAddCode(builder, int32(code))
	transport.TransportResponseAddMessage(builder, msgOff)
	respOff := transport.TransportResponseEnd(builder)
	builder.Finish(respOff)

	resp := transport.GetRootAsTransportResponse(builder.FinishedBytes(), 0)
	return resp
}

func TransportResponseToRaw(resp *transport.TransportResponse) ([]byte, error) {
	if resp == nil {
		return nil, fmt.Errorf("error in TransportResponseToRaw")
	}
	builder := flatbuffers.NewBuilder(0)
	respOff := builder.CreateByteVector(resp.ResponseBytes())
	msgOff := builder.CreateString(string(resp.Message()))

	transport.TransportResponseStart(builder)
	transport.TransportResponseAddId(builder, resp.Id())
	transport.TransportResponseAddCode(builder, resp.Code())
	transport.TransportResponseAddMessage(builder, msgOff)
	transport.TransportResponseAddResponse(builder, respOff)
	respOff = transport.TransportResponseEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportResponse)
	transport.TransportMessageRawAddData(builder, respOff)
	raw := transport.TransportMessageRawEnd(builder)

	builder.Finish(raw)

	data := builder.FinishedBytes()
	return data, nil
}

func RPCSignalToRaw(method transport.Signal, data []byte) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)
	dataOff := builder.CreateByteVector(data)

	transport.TransportSignalStart(builder)
	transport.TransportSignalAddMethod(builder, method)
	transport.TransportSignalAddPayload(builder, dataOff)
	signalOff := transport.TransportSignalEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportSignal)
	transport.TransportMessageRawAddData(builder, signalOff)
	raw := transport.TransportMessageRawEnd(builder)

	builder.Finish(raw)

	res := builder.FinishedBytes()
	return res, nil
}

func RPCBufferResquestToRaw(id int64, method transport.Method,
	req_buffer []byte) ([]byte, error) {
	if len(req_buffer) == 0 {
		return nil, fmt.Errorf("error in RPCBufferResponseToRaw")
	}
	builder := flatbuffers.NewBuilder(len(req_buffer) + 512)
	reqBytesOff := builder.CreateByteVector(req_buffer)

	transport.TransportRequestStart(builder)
	transport.TransportRequestAddId(builder, id)
	transport.TransportRequestAddMethod(builder, method)
	transport.TransportRequestAddRequest(builder, reqBytesOff)
	reqOff := transport.TransportRequestEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportRequest)
	transport.TransportMessageRawAddData(builder, reqOff)
	raw := transport.TransportMessageRawEnd(builder)
	builder.Finish(raw)

	data := builder.FinishedBytes()
	return data, nil
}

func RPCBufferResponseToRaw(id int64, resp_buffer []byte) ([]byte, error) {
	if len(resp_buffer) == 0 {
		return nil, fmt.Errorf("error in RPCBufferResponseToRaw")
	}
	builder := flatbuffers.NewBuilder(len(resp_buffer) + 512)
	respBytesOff := builder.CreateByteVector(resp_buffer)

	transport.TransportResponseStart(builder)
	transport.TransportResponseAddId(builder, id)
	transport.TransportResponseAddResponse(builder, respBytesOff)
	respOff := transport.TransportResponseEnd(builder)

	transport.TransportMessageRawStart(builder)
	transport.TransportMessageRawAddDataType(builder,
		transport.TransportMessageRaw_DataTransportResponse)
	transport.TransportMessageRawAddData(builder, respOff)
	raw := transport.TransportMessageRawEnd(builder)
	builder.Finish(raw)

	data := builder.FinishedBytes()
	return data, nil
}
