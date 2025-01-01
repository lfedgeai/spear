#!/usr/bin/env python3
import logging
import socket
import struct
import threading

import flatbuffers as fbs

from spear.proto.chat import (ChatCompletionRequest, ChatCompletionResponse,
                              ChatMessage, ChatMetadata)
from spear.proto.io import InputRequest, InputResponse
from spear.proto.transform import (TransformRequest, TransformRequest_Params,
                                   TransformResponse, TransformResponse_Data)
from spear.proto.transport import (Method, TransportMessageRaw,
                                   TransportMessageRaw_Data, TransportRequest,
                                   TransportResponse)

MAX_INFLIGHT_REQUESTS = 128
DEFAULT_MESSAGE_SIZE = 4096
TEST_SERVER_DEFAULT_PORT = 12345
TEST_SERVER_DEFAULT_SECRET = 12345

logger = logging.getLogger(__name__)
logger.setLevel(logging.DEBUG)


class TestAgentServer:
    """
    A test tcp server for testing the agent
    """

    def __init__(
        self,
        port: int = TEST_SERVER_DEFAULT_PORT,
        secret: int = TEST_SERVER_DEFAULT_SECRET,
    ):
        self._server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._server.bind(("localhost", port))
        self._server.listen(5)
        self._client = None
        self._secret = secret

    def run(self):
        """
        run the server
        """
        while True:
            client, _ = self._server.accept()
            # get the secret
            data = client.recv(8)
            secret = struct.unpack("<Q", data)
            # make sure the secret is correct
            if secret[0] != self._secret:
                logger.error("Invalid secret")
                client.close()
                continue
            self._client = client
            break
        self._handle_client()

    def _handle_client(self):
        """
        handle the client
        """
        # set blocking to true
        while True:
            data = self._client.recv(8)
            length = struct.unpack("<Q", data)
            data = self._client.recv(length[0])
            self._process_data(data)

    def _send_transform_response(self, req_id: int):
        try:
            builder = fbs.Builder(DEFAULT_MESSAGE_SIZE)
            # create chat completion response
            ChatMetadata.ChatMetadataStart(builder)
            ChatMetadata.AddRole(builder, 0)
            ChatMetadata.AddReason(builder, 2)
            metadata_off = ChatMetadata.End(builder)

            restext = builder.CreateString("Hi there")

            ChatMessage.ChatMessageStart(builder)
            ChatMessage.AddContent(builder, restext)
            ChatMessage.AddMetadata(builder, metadata_off)
            msg_off = ChatMessage.End(builder)

            ChatCompletionResponse.ChatCompletionResponseStartMessagesVector(builder, 1)
            builder.PrependUOffsetTRelative(msg_off)
            msglist_off = builder.EndVector()

            ChatCompletionResponse.ChatCompletionResponseStart(builder)
            ChatCompletionResponse.AddMessages(builder, msglist_off)
            chatcomp_off = ChatCompletionResponse.End(builder)

            TransformResponse.TransformResponseStart(builder)
            TransformResponse.AddData(builder, chatcomp_off)
            TransformResponse.AddDataType(
                builder,
                TransformResponse_Data.TransformResponse_Data.spear_proto_chat_ChatCompletionResponse,
            )
            builder.Finish(TransformResponse.End(builder))

            builder2 = fbs.Builder(DEFAULT_MESSAGE_SIZE)

            # add builder.Output() to builder2
            resp_off = builder2.CreateByteVector(builder.Output())

            TransportResponse.TransportResponseStart(builder2)
            TransportResponse.AddId(builder2, req_id)
            TransportResponse.AddCode(builder2, 0)
            TransportResponse.AddResponse(builder2, resp_off)
            trasport_off = TransportResponse.End(builder2)

            TransportMessageRaw.TransportMessageRawStart(builder2)
            TransportMessageRaw.TransportMessageRawAddDataType(
                builder2,
                TransportMessageRaw_Data.TransportMessageRaw_Data.TransportResponse,
            )
            TransportMessageRaw.TransportMessageRawAddData(builder2, trasport_off)
            builder2.Finish(TransportMessageRaw.TransportMessageRawEnd(builder2))

            raw_resp = builder2.Output()
            sz = len(raw_resp).to_bytes(8, byteorder="little")
            self._client.sendall(sz)
            self._client.sendall(raw_resp)
        except Exception as e:
            logger.error("Error sending response: %s", str(e))

    def _process_transform_request(self, transform_req: TransformRequest, req_id: int):
        if (
            transform_req.ParamsType()
            == TransformRequest_Params.TransformRequest_Params.spear_proto_chat_ChatCompletionRequest
        ):
            chat_req = ChatCompletionRequest.ChatCompletionRequest()
            chat_req.Init(transform_req.Params().Bytes, transform_req.Params().Pos)
            for i in range(chat_req.MessagesLength()):
                logger.info(
                    "Message: %s", chat_req.Messages(i).Content().decode("utf-8")
                )
            # create a thread to send the response
            t = threading.Thread(target=self._send_transform_response, args=(req_id,))
            t.daemon = True
            t.start()
        else:
            logger.error("Invalid params type")

    def _send_input_response(self, req_id: int):
        try:
            builder2 = fbs.Builder(DEFAULT_MESSAGE_SIZE)
            res = builder2.CreateString("abcdefghijklmnopqrstuvwxyz")
            InputResponse.InputResponseStart(builder2)
            InputResponse.InputResponseAddText(builder2, res)
            data_off = InputResponse.InputResponseEnd(builder2)
            builder2.Finish(data_off)
            resp_data = builder2.Output()

            builder = fbs.Builder(DEFAULT_MESSAGE_SIZE)
            # create vector
            resp_off = builder.CreateByteVector(resp_data)
            TransportResponse.TransportResponseStart(builder)
            TransportResponse.AddId(builder, req_id)
            TransportResponse.AddCode(builder, 0)
            TransportResponse.AddResponse(builder, resp_off)
            trasport_off = TransportResponse.End(builder)

            TransportMessageRaw.TransportMessageRawStart(builder)
            TransportMessageRaw.TransportMessageRawAddDataType(
                builder,
                TransportMessageRaw_Data.TransportMessageRaw_Data.TransportResponse,
            )
            TransportMessageRaw.TransportMessageRawAddData(builder, trasport_off)
            builder.Finish(TransportMessageRaw.TransportMessageRawEnd(builder))

            raw_resp = builder.Output()
            sz = len(raw_resp).to_bytes(8, byteorder="little")
            self._client.sendall(sz)
            self._client.sendall(raw_resp)
        except Exception as e:
            logger.error("Error sending response: %s", str(e))

    def _process_input_request(self, input_req: InputRequest, req_id: int):
        logger.info("Prompt: %s", input_req.Prompt().decode("utf-8"))
        self._send_input_response(req_id)

    def _process_transport_request(self, req: TransportRequest, req_id: int):
        if req.Method() == Method.Method.Transform:
            # convert from TransportRequest to TransformRequest
            transform_req = TransformRequest.TransformRequest.GetRootAsTransformRequest(
                req.RequestAsNumpy(), 0
            )
            # convert params to ChatCompletionRequest
            self._process_transform_request(transform_req, req_id)
        elif req.Method() == Method.Method.Input:
            input_req = InputRequest.InputRequest.GetRootAsInputRequest(
                req.RequestAsNumpy(), 0
            )
            self._process_input_request(input_req, req_id)
        else:
            logger.error("Invalid method")
            raise ValueError("Invalid method")

    def _process_data(self, data: bytes):
        raw = TransportMessageRaw.TransportMessageRaw.GetRootAsTransportMessageRaw(data)
        if (
            raw.DataType()
            == TransportMessageRaw_Data.TransportMessageRaw_Data.TransportRequest
        ):
            req = TransportRequest.TransportRequest()
            req.Init(raw.Data().Bytes, raw.Data().Pos)
            self._process_transport_request(req, req.Id())
        elif (
            raw.DataType()
            == TransportMessageRaw_Data.TransportMessageRaw_Data.TransportResponse
        ):
            resp = TransportResponse.TransportResponse()
            resp.Init(raw.Data().Bytes, raw.Data().Pos)
            logger.info("Response: %d", resp.Id())
